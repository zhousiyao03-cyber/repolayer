//! Static-embedding model loader (model2vec / potion-code-16M).
//!
//! `Embedder::open(model_dir)` mmaps `model.safetensors`, loads `tokenizer.json`
//! via the HuggingFace `tokenizers` crate, and exposes `encode_one(text)` which
//! returns a normalized `[f32; DIM]` for a single string.
//!
//! The model is a "static" embedder: no neural-net inference, just a `vocab × dim`
//! float32 matrix. Encoding is `tokenize → mean-pool → L2-normalize`. Cost per
//! call is dominated by tokenization (~10–100 µs depending on string length);
//! the embedding lookup itself is essentially free.

use memmap2::Mmap;
use safetensors::{Dtype, SafeTensors};
use std::fs::File;
use std::io;
use std::path::Path;
use std::sync::Arc;
use tokenizers::Tokenizer;

/// Output dimension of `potion-code-16M`. Embedded as a const so callers can
/// stack-allocate result buffers.
pub const DIM: usize = 256;

/// Best-effort: encode a query string using the cached potion-code-16M
/// model. Returns `None` when the model isn't on disk (so callers can
/// fall back to BM25-only search). Never triggers a download.
pub fn try_encode_query(query: &str) -> Option<Vec<f32>> {
    use crate::search::download::{model_dir, ModelInfo};

    let info = ModelInfo::potion_code_16m();
    let dir = model_dir(&info).ok()?;
    if !dir.join("model.safetensors").is_file() || !dir.join("tokenizer.json").is_file() {
        return None;
    }
    let embedder = Embedder::open(&dir).ok()?;
    Some(embedder.encode_one(query).to_vec())
}

/// Tensor name inside `model.safetensors`. model2vec's convention.
const EMBEDDINGS_TENSOR: &str = "embeddings";

pub struct Embedder {
    /// Keep the mmap alive for the lifetime of the embedder so the embedding
    /// slice stays valid.
    _mmap: Arc<Mmap>,
    /// Borrow into `_mmap`: vocab_size × DIM rows of f32, row-major.
    /// Stored as a raw pointer + length so `Embedder` can be `Send + Sync`.
    embeddings_ptr: *const f32,
    vocab_size: usize,
    tokenizer: Tokenizer,
}

// SAFETY: the underlying bytes are immutable mmap'd file data; reads from many
// threads are safe.
unsafe impl Send for Embedder {}
unsafe impl Sync for Embedder {}

impl Embedder {
    /// Open the cached model files from `model_dir`. Expects:
    /// - `<model_dir>/model.safetensors` containing a single `f32` tensor named
    ///   `embeddings` with shape `[vocab_size, DIM]`.
    /// - `<model_dir>/tokenizer.json` in HuggingFace tokenizers format.
    pub fn open(model_dir: &Path) -> io::Result<Self> {
        let safetensors_path = model_dir.join("model.safetensors");
        let tokenizer_path = model_dir.join("tokenizer.json");

        let file = File::open(&safetensors_path)?;
        let mmap = unsafe { Mmap::map(&file) }?;
        let mmap = Arc::new(mmap);

        // Parse the safetensors header against the same byte slice the matrix
        // lives in, then extract the embeddings tensor.
        let st = SafeTensors::deserialize(&mmap[..])
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("safetensors: {e}")))?;
        let names: Vec<&str> = st.names().into_iter().collect();
        let tensor = st.tensor(EMBEDDINGS_TENSOR).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "safetensors missing '{EMBEDDINGS_TENSOR}' tensor (have: {names:?}): {e}"
                ),
            )
        })?;
        if tensor.dtype() != Dtype::F32 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("expected F32 embeddings, got {:?}", tensor.dtype()),
            ));
        }
        let shape = tensor.shape();
        if shape.len() != 2 || shape[1] != DIM {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("expected shape [V, {DIM}], got {shape:?}"),
            ));
        }
        let vocab_size = shape[0];
        let data = tensor.data();
        if data.len() != vocab_size * DIM * 4 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "tensor data length {} != {} = vocab_size * DIM * 4",
                    data.len(),
                    vocab_size * DIM * 4
                ),
            ));
        }
        // SAFETY: mmap'd file data; safetensors guarantees the byte length is a
        // multiple of size_of::<f32>(); 4-byte alignment is guaranteed because
        // mmap always returns page-aligned pointers and the safetensors header
        // pads the data offset to a multiple of 8 (so adding the offset keeps
        // 4-byte alignment).
        let ptr = data.as_ptr() as *const f32;

        debug_assert_eq!(
            (ptr as usize) % std::mem::align_of::<f32>(),
            0,
            "embedding tensor must be f32-aligned"
        );

        let tokenizer = Tokenizer::from_file(&tokenizer_path).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("tokenizer.json: {e}"),
            )
        })?;

        Ok(Self {
            _mmap: mmap,
            embeddings_ptr: ptr,
            vocab_size,
            tokenizer,
        })
    }

    #[allow(dead_code)] // used by network-gated tests
    pub fn vocab_size(&self) -> usize {
        self.vocab_size
    }

    /// All embedding rows as one big slice. Read-only; backed by the mmap.
    fn all_rows(&self) -> &[f32] {
        // SAFETY: `embeddings_ptr` is valid for `vocab_size * DIM` f32s for the
        // lifetime of `self._mmap`.
        unsafe {
            std::slice::from_raw_parts(self.embeddings_ptr, self.vocab_size * DIM)
        }
    }

    /// Look up the embedding row for a single token id. OOV ids are clamped
    /// to the unknown-token row 0 (model2vec convention: vocab[0] is `[UNK]`).
    fn row(&self, token_id: u32) -> &[f32] {
        let id = (token_id as usize).min(self.vocab_size.saturating_sub(1));
        let start = id * DIM;
        &self.all_rows()[start..start + DIM]
    }

    /// Encode one string into a normalized `[f32; DIM]`.
    ///
    /// Implements model2vec's pipeline: tokenize → mean-pool → L2-normalize.
    /// Empty input (or input that tokenizes to zero ids) returns the zero vector.
    pub fn encode_one(&self, text: &str) -> [f32; DIM] {
        let mut out = [0.0f32; DIM];
        if text.is_empty() {
            return out;
        }

        let encoding = match self.tokenizer.encode(text, /* add_special_tokens */ false) {
            Ok(e) => e,
            Err(_) => return out,
        };
        let ids = encoding.get_ids();
        if ids.is_empty() {
            return out;
        }

        // Sum embeddings.
        for &id in ids {
            let row = self.row(id);
            for i in 0..DIM {
                out[i] += row[i];
            }
        }
        // Mean-pool.
        let inv_n = 1.0 / ids.len() as f32;
        for v in &mut out {
            *v *= inv_n;
        }
        // L2-normalize.
        let norm: f32 = out.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm > 0.0 {
            let inv = 1.0 / norm;
            for v in &mut out {
                *v *= inv;
            }
        }
        out
    }

}

// ─────────────────────────────────────────────────────────────────────────
// Brute-force cosine top-k over a chunk-embedding matrix.
// ─────────────────────────────────────────────────────────────────────────

/// Threshold above which we parallelise the scan via rayon. Below this many
/// rows the per-task overhead exceeds the win.
const PAR_THRESHOLD: usize = 4096;

/// Score every row of `embeddings` against `query` and return the top-k by
/// cosine similarity (descending).
///
/// `embeddings` is the row-major `n × DIM` chunk-embedding matrix produced
/// at index time. Both `query` and every row are assumed to be L2-normalized
/// (which `Embedder::encode_one` guarantees), so cosine reduces to a dot
/// product.
///
/// `mask`, if provided, is a `Vec<bool>` of length `n`. Rows where the mask
/// is `false` are scored as `-INFINITY` and never appear in the result.
/// This matches `bm25.get_scores`'s post-filter-weight semantics — we drop
/// them entirely rather than zeroing, since 0 may legitimately rank.
///
/// Returns up to `k` `(row_id, score)` pairs sorted by score descending.
pub fn cosine_topk(
    query: &[f32; DIM],
    embeddings: &[f32],
    mask: Option<&[bool]>,
    k: usize,
) -> Vec<(u32, f32)> {
    use rayon::prelude::*;

    let n = embeddings.len() / DIM;
    debug_assert_eq!(embeddings.len() % DIM, 0, "embeddings length not a multiple of DIM");
    if let Some(m) = mask {
        debug_assert_eq!(m.len(), n, "mask length must equal row count");
    }
    if n == 0 || k == 0 {
        return Vec::new();
    }

    // Pre-load the query into 32 × f32x8 SIMD lanes so each row's dot product
    // does no extra work to splat the query.
    let q_lanes = load_query_lanes(query);

    // Score every row. Parallel for big matrices, single-threaded otherwise.
    let scores: Vec<f32> = if n >= PAR_THRESHOLD {
        (0..n)
            .into_par_iter()
            .with_min_len(256)
            .map(|i| score_row(i, embeddings, &q_lanes, mask))
            .collect()
    } else {
        (0..n)
            .map(|i| score_row(i, embeddings, &q_lanes, mask))
            .collect()
    };

    // Top-k via partial sort. For small k a min-heap is theoretically faster
    // (O(n log k) vs O(n log n)), but n is typically ~10k and k ≤ 50, so a
    // simple `select_nth_unstable_by` followed by sorting the prefix is
    // simpler and cache-friendly enough.
    let mut idx: Vec<u32> = (0..n as u32).collect();
    let take = k.min(n);
    idx.select_nth_unstable_by(take - 1, |&a, &b| {
        scores[b as usize]
            .partial_cmp(&scores[a as usize])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut top: Vec<u32> = idx.into_iter().take(take).collect();
    top.sort_by(|&a, &b| {
        scores[b as usize]
            .partial_cmp(&scores[a as usize])
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    top.into_iter()
        .filter_map(|i| {
            let s = scores[i as usize];
            if s.is_finite() { Some((i, s)) } else { None }
        })
        .collect()
}

#[inline]
fn score_row(
    i: usize,
    embeddings: &[f32],
    q_lanes: &[wide::f32x8; DIM / 8],
    mask: Option<&[bool]>,
) -> f32 {
    if let Some(m) = mask {
        if !m[i] {
            return f32::NEG_INFINITY;
        }
    }
    let row = &embeddings[i * DIM..(i + 1) * DIM];
    dot_simd(q_lanes, row)
}

#[inline]
fn load_query_lanes(query: &[f32; DIM]) -> [wide::f32x8; DIM / 8] {
    let mut out = [wide::f32x8::splat(0.0); DIM / 8];
    for (i, lane) in out.iter_mut().enumerate() {
        let chunk: [f32; 8] = query[i * 8..(i + 1) * 8].try_into().unwrap();
        *lane = wide::f32x8::from(chunk);
    }
    out
}

/// SIMD dot product of a query (pre-loaded into 32 lanes) and a row slice.
/// Both operands are L2-normalized, so the dot product equals cosine.
#[inline]
fn dot_simd(q_lanes: &[wide::f32x8; DIM / 8], row: &[f32]) -> f32 {
    debug_assert_eq!(row.len(), DIM);
    let mut acc = wide::f32x8::splat(0.0);
    for (i, q) in q_lanes.iter().enumerate() {
        let chunk: [f32; 8] = row[i * 8..(i + 1) * 8].try_into().unwrap();
        let r = wide::f32x8::from(chunk);
        acc += *q * r;
    }
    // Horizontal sum.
    let arr: [f32; 8] = acc.into();
    arr.iter().sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::download::{ensure_model, ModelInfo};

    fn ensure_real_model() -> std::path::PathBuf {
        // Download once into a per-test cache; subsequent runs reuse the cache
        // and the SHA-256 manifest verification fast-paths.
        let info = ModelInfo::potion_code_16m();
        ensure_model(&info).expect("model download failed; see network-security wiki")
    }

    #[test]
    #[ignore]
    fn network_loads_potion_model() {
        let dir = ensure_real_model();
        let emb = Embedder::open(&dir).expect("Embedder::open failed");
        assert!(emb.vocab_size() > 1000, "vocab implausibly small");
    }

    #[test]
    #[ignore]
    fn network_encodes_to_unit_vector() {
        let emb = Embedder::open(&ensure_real_model()).unwrap();

        let v = emb.encode_one("def parse_json(s): return json.loads(s)");
        // Every component should be finite.
        for x in v.iter() {
            assert!(x.is_finite(), "non-finite component: {x}");
        }
        // L2-norm should be ~1 (we just normalized).
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5, "norm = {norm}, expected ≈ 1.0");
    }

    #[test]
    #[ignore]
    fn network_similar_strings_have_high_cosine() {
        // Sanity: two semantically-similar code snippets should be closer than
        // unrelated ones. Doesn't pin to specific scores — just enforces ordering.
        let emb = Embedder::open(&ensure_real_model()).unwrap();
        let a = emb.encode_one("def add(a, b): return a + b");
        let b = emb.encode_one("def sum(x, y): return x + y");
        let c = emb.encode_one("class HttpServer: def listen(self, port): pass");

        let cos = |u: &[f32], v: &[f32]| -> f32 {
            u.iter().zip(v.iter()).map(|(x, y)| x * y).sum::<f32>()
        };
        let ab = cos(&a, &b);
        let ac = cos(&a, &c);
        assert!(
            ab > ac,
            "expected related code (cos {ab}) > unrelated code (cos {ac})"
        );
    }

    // ── cosine_topk (pure unit tests, no network) ──────────────────────────

    fn unit(values: &[f32]) -> Vec<f32> {
        let mut v = values.to_vec();
        v.resize(DIM, 0.0);
        let n: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if n > 0.0 {
            for x in &mut v {
                *x /= n;
            }
        }
        v
    }

    #[test]
    fn cosine_topk_empty_returns_empty() {
        let q = [0.0f32; DIM];
        assert!(cosine_topk(&q, &[], None, 5).is_empty());
    }

    #[test]
    fn cosine_topk_zero_k_returns_empty() {
        let q = [0.0f32; DIM];
        let rows = vec![0.0f32; DIM];
        assert!(cosine_topk(&q, &rows, None, 0).is_empty());
    }

    #[test]
    fn cosine_topk_orders_by_similarity() {
        // Three rows along ±axes; query along +x. Best match is row 0.
        let mut rows = Vec::new();
        rows.extend(unit(&[1.0, 0.0, 0.0])); // row 0: same direction as q
        rows.extend(unit(&[0.0, 1.0, 0.0])); // row 1: orthogonal
        rows.extend(unit(&[-1.0, 0.0, 0.0])); // row 2: opposite

        let q: [f32; DIM] = unit(&[1.0, 0.0, 0.0]).try_into().unwrap();
        let top = cosine_topk(&q, &rows, None, 3);

        assert_eq!(top.len(), 3);
        assert_eq!(top[0].0, 0);
        assert!((top[0].1 - 1.0).abs() < 1e-5);
        assert_eq!(top[1].0, 1);
        assert!(top[1].1.abs() < 1e-5); // ≈ 0
        assert_eq!(top[2].0, 2);
        assert!((top[2].1 + 1.0).abs() < 1e-5); // ≈ -1
    }

    #[test]
    fn cosine_topk_respects_k() {
        let mut rows = Vec::new();
        for i in 0..10 {
            // Each row aligns with query proportionally — row 0 best, row 9 worst.
            let mag = 1.0 - (i as f32) * 0.1;
            rows.extend(unit(&[mag, 0.1, 0.0]));
        }
        let q: [f32; DIM] = unit(&[1.0, 0.0, 0.0]).try_into().unwrap();
        let top = cosine_topk(&q, &rows, None, 3);
        assert_eq!(top.len(), 3);
        // Top-3 should be rows 0, 1, 2 in order.
        assert_eq!(top[0].0, 0);
        assert_eq!(top[1].0, 1);
        assert_eq!(top[2].0, 2);
    }

    #[test]
    fn cosine_topk_mask_excludes_filtered_rows() {
        let mut rows = Vec::new();
        rows.extend(unit(&[1.0, 0.0, 0.0])); // row 0: best
        rows.extend(unit(&[0.9, 0.1, 0.0])); // row 1: second best
        rows.extend(unit(&[0.8, 0.2, 0.0])); // row 2: third best

        let q: [f32; DIM] = unit(&[1.0, 0.0, 0.0]).try_into().unwrap();
        // Mask out the top row.
        let mask = vec![false, true, true];
        let top = cosine_topk(&q, &rows, Some(&mask), 5);
        // Should return rows 1 and 2 only — row 0 was filtered before scoring.
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].0, 1);
        assert_eq!(top[1].0, 2);
    }

    #[test]
    fn cosine_topk_handles_large_matrix() {
        let n = 5000;
        let mut rows = Vec::with_capacity(n * DIM);
        for i in 0..n {
            // Make row i best at "index i" with falling similarity to q (which is row 0).
            let mut v = vec![0.0f32; 3];
            v[0] = 1.0 - (i as f32) / (n as f32);
            v[1] = (i as f32) / (n as f32);
            rows.extend(unit(&v));
        }
        let q: [f32; DIM] = unit(&[1.0, 0.0, 0.0]).try_into().unwrap();
        let top = cosine_topk(&q, &rows, None, 5);
        assert_eq!(top.len(), 5);
        // Top result should be row 0 (full alignment with q).
        assert_eq!(top[0].0, 0);
        // Scores should be monotonically non-increasing.
        for w in top.windows(2) {
            assert!(w[0].1 >= w[1].1, "scores not monotone: {:?}", top);
        }
    }

    #[test]
    #[ignore]
    fn network_empty_returns_zero_vector() {
        let emb = Embedder::open(&ensure_real_model()).unwrap();
        let v = emb.encode_one("");
        assert!(v.iter().all(|&x| x == 0.0));
    }
}

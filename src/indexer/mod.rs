pub mod incremental;

use crate::config::Config;
use crate::deps::store::DepStore;
use crate::graph::model::*;
use crate::graph::store::Store;
use crate::outline::store::OutlineStore;
use crate::search::store::SearchStore;
use anyhow::{Context, Result};
use ignore::WalkBuilder;
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

/// Normalize a relative path to use `/` separators for stable cross-platform IDs.
fn rel_path_str(rel: &Path) -> String {
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

pub struct Indexer {
    pub workspace_root: PathBuf,
    pub store: Store,
    pub outline_store: OutlineStore,
    pub deps_store: DepStore,
    pub search_store: SearchStore,
    pub config: Config,
}

#[derive(Debug, Default)]
pub struct BuildStats {
    pub nodes: u64,
    pub edges: u64,
    /// HTTP embedding requests that returned a successful response. Zero for
    /// non-HTTP providers (potion-local / legacy).
    pub embed_requests: u64,
    /// HTTP embedder retry count (each retry attempt, not failed batches).
    pub embed_retries: u64,
    /// Total input characters sent to the embedder across the build. Useful
    /// for cost estimation: providers typically charge per ~4 chars-per-token.
    pub embed_input_chars: u64,
    /// Number of dense vectors the embedder returned (≈ chunk count when
    /// every batch succeeds).
    pub embed_vectors: u64,
    /// Number of (file, scope=module) summaries persisted into summary_chunks.
    pub summary_count: u64,
}

impl Indexer {
    /// Open all 4 SQLite stores under the same `.repolayer/` directory that
    /// `db_path` lives in. `db_path` is expected to be `<workspace>/.repolayer/index.db`.
    pub fn new(workspace_root: PathBuf, db_path: PathBuf, config: Config) -> Result<Self> {
        let store = Store::open(&db_path)
            .with_context(|| format!("opening store at {}", db_path.display()))?;

        let dir = db_path.parent().unwrap_or(Path::new(".")).to_path_buf();
        let outline_store = OutlineStore::open(&dir.join("outline.db"))
            .with_context(|| format!("opening outline.db at {}", dir.display()))?;
        let deps_store = DepStore::open(&dir.join("deps.db"))
            .with_context(|| format!("opening deps.db at {}", dir.display()))?;

        // Target embedding dim — from config if explicitly set, otherwise legacy potion DIM.
        let dim = config
            .embedding
            .as_ref()
            .map(|e| e.dim as usize)
            .unwrap_or(crate::search::embed::DIM);
        let search_store =
            crate::search::store::SearchStore::open_with_dim(&dir.join("search.db"), dim)
                .with_context(|| format!("opening search.db at {}", dir.display()))?;

        Ok(Self {
            workspace_root,
            store,
            outline_store,
            deps_store,
            search_store,
            config,
        })
    }

    pub async fn build_all(&mut self) -> Result<BuildStats> {
        let mut stats = BuildStats::default();

        // ── Phase A — parse + write each repo ─────────────────────────────────
        // We need pkg_index for cross-repo import resolution inside index_repo_v2.
        let pkg_index =
            crate::linker::imports::PackageIndex::build(&self.workspace_root, &self.config)?;

        let repos = self.config.repos.clone();
        let mut code_repos: Vec<(String, PathBuf)> = Vec::new();

        for repo_cfg in &repos {
            let repo_path = self.resolve_repo_path(&repo_cfg.path);
            let repo_name = repo_cfg.name.clone().unwrap_or_else(|| {
                repo_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "repo".to_string())
            });
            if repo_cfg.is_idl() {
                self.index_idl_repo(&repo_name, &repo_path, &mut stats)?;
                continue;
            }
            self.index_repo_v2(&repo_name, &repo_path, &pkg_index, &mut stats)?;
            code_repos.push((repo_name, repo_path));
        }

        // ── Phase B — cross-repo / IDL gluing (serial) ────────────────────────

        // PackageIndex already built above for import resolution during Phase A.
        // No additional cross-repo import pass needed here — import edges were
        // inserted per-file during Phase A.

        // deps::build_for_repo per non-IDL repo → DepStore
        for (repo_name, repo_path) in &code_repos {
            match crate::deps::build_for_repo(repo_path) {
                Ok(graph) => {
                    if let Err(e) = self.deps_store.replace_repo_graph(repo_name, &graph) {
                        warn!("deps_store write failed for {}: {}", repo_name, e);
                    }
                }
                Err(e) => warn!("deps::build_for_repo({}) failed: {}", repo_name, e),
            }
        }

        // IdlLinker: Implements/Invokes edges in main graph
        let linker = crate::linker::idl_links::IdlLinker {
            store: &self.store,
            repos: code_repos.clone(),
        };
        let idl_edges = linker.link_all()?;
        stats.edges += idl_edges;

        // CallsLinker: Module→Function/Method Calls edges via ast-grep.
        // Only links callee names that resolve to exactly one Function/Method
        // node workspace-wide, so confidence is 1.0 by construction.
        let calls_linker = crate::linker::calls::CallsLinker {
            store: &self.store,
            repos: code_repos.clone(),
        };
        match calls_linker.link_all() {
            Ok(n) => {
                info!("calls: {} unique-resolution Calls edges", n);
                stats.edges += n;
            }
            Err(e) => warn!("calls linker failed: {}", e),
        }

        // imports_to_repo: high-confidence Imports edges from real Go module
        // imports (deps.db.external_imports → matching workspace repo).
        match crate::linker::imports_to_repo::link(
            &self.store,
            &self.deps_store,
            &self.workspace_root,
            &self.config,
        ) {
            Ok(n) => {
                info!("imports_to_repo: {} cross-repo Imports edges", n);
                stats.edges += n;
            }
            Err(e) => warn!("imports_to_repo failed: {}", e),
        }

        // Manual links from repolayer.yml
        let manual_edges = crate::linker::manual::apply_manual_links(&self.store, &self.config)?;
        stats.edges += manual_edges;

        // ── Phase C — search index (chunks + dense embeddings) ────────────────
        // Step 1: write chunks for every repo.
        for (repo_name, repo_path) in &code_repos {
            let files = match self.outline_store.list_files(repo_name) {
                Ok(f) => f,
                Err(e) => {
                    warn!("outline_store.list_files({}) failed: {}", repo_name, e);
                    continue;
                }
            };
            let mut all_chunks = Vec::new();
            for (_repo, rel) in &files {
                let abs = repo_path.join(rel);
                // chunk_file returns empty vec for unsupported extensions; safe to call always
                let chunks = crate::search::chunker::chunk_file(&abs, rel);
                all_chunks.extend(chunks);
            }
            if let Err(e) = self
                .search_store
                .replace_repo_chunks(repo_name, &all_chunks)
            {
                warn!("search_store write failed for {}: {}", repo_name, e);
            }
        }

        // Step 2: try to embed every chunk.
        //
        // Policy: download the ~64 MB potion-code-16M model only when the user
        // has opted in with `REPOLAYER_DOWNLOAD=1`. Once the model is in the
        // cache (or AST_OUTLINE_MODEL_DIR points to one), subsequent builds
        // embed automatically without the env var. If the model isn't present,
        // we log a one-line hint and fall through — search will use the
        // BM25/substring path.
        match try_embed_async(
            &mut self.search_store,
            &code_repos,
            self.config.embedding.as_ref(),
            &mut stats,
        )
        .await
        {
            EmbedOutcome::Done(n) => info!("embedded {} chunks into search.db", n),
            EmbedOutcome::Skipped(reason) => {
                info!(
                    "embedding step skipped ({}); search will fall back to BM25/substring. \
                     Set REPOLAYER_DOWNLOAD=1 to fetch the embedding model (~64 MB).",
                    reason
                );
            }
            EmbedOutcome::Failed(err) => {
                warn!(
                    "embedding step failed ({}); search will fall back to BM25/substring",
                    err
                );
            }
        }

        // ── Phase D — optional LLM summary lane in search ─────────────────────
        if let (Some(emb_cfg), Some(sum_cfg)) =
            (self.config.embedding.as_ref(), self.config.summary.as_ref())
        {
            if sum_cfg.enabled {
                match crate::llm::summary_store::run_summary_phase(
                    &self.search_store,
                    &code_repos,
                    emb_cfg,
                    sum_cfg,
                )
                .await
                {
                    Ok(n) => {
                        info!("summary lane: wrote {n} summaries");
                        stats.summary_count = n as u64;
                    }
                    Err(e) => warn!("summary phase failed: {e}"),
                }
            }
        }

        // Legacy: preserve module-summary-in-main-graph path (used by other tools).
        if let Some(llm_cfg) = &self.config.llm.clone() {
            if llm_cfg.enabled && llm_cfg.summary {
                match build_llm_provider(llm_cfg) {
                    Ok(provider) => {
                        if let Err(e) = crate::llm::summary::summarize_modules(
                            &self.store,
                            provider,
                            &code_repos,
                        )
                        .await
                        {
                            warn!("legacy LLM summary phase failed: {}", e);
                        }
                    }
                    Err(e) => warn!("legacy LLM provider construction failed: {}", e),
                }
            }
        }

        // Replace emit-counts with authoritative DB counts.
        // upsert_{node,edge} are idempotent, so the running counters above
        // overcount whenever the same id is written more than once (e.g.
        // a target Module first synthesized by an Imports edge and then
        // re-walked as a real source file). Reading from SQLite at the end
        // gives the user the true graph size.
        stats.nodes = self.store.count_nodes()? as u64;
        stats.edges = self.store.count_edges()? as u64;

        Ok(stats)
    }

    /// Phase A: walk files in parallel (rayon), collect ParseResults, then
    /// serially write to index.db + outline.db. Cross-repo import edges are
    /// also resolved and written during the serial write phase.
    fn index_repo_v2(
        &mut self,
        repo: &str,
        root: &Path,
        pkg_index: &crate::linker::imports::PackageIndex,
        stats: &mut BuildStats,
    ) -> Result<()> {
        info!("indexing repo {} at {}", repo, root.display());
        let repo_node = Node::new(NodeKind::Repo, repo, "", None);
        self.store.upsert_node(&repo_node)?;
        stats.nodes += 1;

        // Collect all file paths first (serial walk is fast; the parse is the bottleneck)
        let entries: Vec<PathBuf> = WalkBuilder::new(root)
            .build()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
            .map(|e| e.path().to_path_buf())
            .collect();

        // Parallel parse phase — adapters::parse_file is the heavy tree-sitter work
        // mpsc channel not needed: par_iter + collect is simpler and avoids Sync issues
        let parsed: Vec<(PathBuf, crate::core::declaration::ParseResult)> = entries
            .par_iter()
            .filter_map(|p| crate::adapters::parse_file(p).map(|pr| (p.clone(), pr)))
            .collect();

        // Serial write phase — single SQLite connection; no concurrent writes needed
        for (abs_path, parse_result) in parsed {
            let rel = rel_path_str(abs_path.strip_prefix(root).unwrap_or(&abs_path));

            // Module node + Contains edge from repo
            let module_node = Node::new(NodeKind::Module, repo, &rel, None);
            self.store.upsert_node(&module_node)?;
            self.store.upsert_edge(&Edge {
                from: repo_node.id.clone(),
                to: module_node.id.clone(),
                kind: EdgeKind::Contains,
                confidence: 1.0,
            })?;
            stats.nodes += 1;
            stats.edges += 1;

            // Emit Type/Method/Function nodes from the Declaration tree
            for decl in &parse_result.declarations {
                emit_decl_nodes(
                    &self.store,
                    &repo_node.id,
                    &module_node.id,
                    repo,
                    &rel,
                    decl,
                    None,
                    stats,
                )?;
            }

            // Resolve imports from the old parser's import list using the source path.
            // NOTE: adapters::parse_file does not return an import list (it returns
            // Declaration trees). For intra-repo and cross-repo import edges we fall
            // back to the legacy parse_by_extension path only for TypeScript/JS files
            // that have a simple relative-import model. Full import-edge wiring is
            // preserved for Plan B; the new adapters provide the richer Declaration tree.
            let imports = extract_imports_for_file(&abs_path);
            for imp in &imports {
                if let Some(target_path) = resolve_import(root, &abs_path, imp) {
                    let target_rel_path = target_path.strip_prefix(root).unwrap_or(&target_path);
                    let target_rel = rel_path_str(target_rel_path);
                    let target_module = Node::new(NodeKind::Module, repo, &target_rel, None);
                    self.store.upsert_node(&target_module)?;
                    self.store.upsert_edge(&Edge {
                        from: repo_node.id.clone(),
                        to: target_module.id.clone(),
                        kind: EdgeKind::Contains,
                        confidence: 1.0,
                    })?;
                    self.store.upsert_edge(&Edge {
                        from: module_node.id.clone(),
                        to: target_module.id,
                        kind: EdgeKind::Imports,
                        confidence: 1.0,
                    })?;
                    stats.edges += 1;
                } else if let Some(pkg) = pkg_index.lookup(imp) {
                    let target_rel = pkg
                        .main_relative
                        .clone()
                        .unwrap_or_else(|| "package.json".to_string());
                    let target_module = Node::new(NodeKind::Module, &pkg.repo, &target_rel, None);
                    self.store.upsert_node(&target_module)?;
                    if pkg.main_relative.is_none() {
                        let target_repo_node = Node::new(NodeKind::Repo, &pkg.repo, "", None);
                        self.store.upsert_edge(&Edge {
                            from: target_repo_node.id,
                            to: target_module.id.clone(),
                            kind: EdgeKind::Contains,
                            confidence: 1.0,
                        })?;
                    }
                    self.store.upsert_edge(&Edge {
                        from: module_node.id.clone(),
                        to: target_module.id,
                        kind: EdgeKind::Imports,
                        confidence: 1.0,
                    })?;
                    stats.edges += 1;
                }
                // External dep not in workspace — skip silently.
            }

            // Outline store: upsert (repo, path) row with full Declaration tree
            let content_hash = hash_source(&parse_result.source);
            self.outline_store
                .upsert(repo, &parse_result, &content_hash)?;
        }

        Ok(())
    }

    fn index_idl_repo(&mut self, repo: &str, root: &Path, stats: &mut BuildStats) -> Result<()> {
        use crate::adapters::idl::{protobuf::ProtobufParser, thrift::ThriftParser};
        let proto_p = ProtobufParser::new();
        let thrift_p = ThriftParser::new();

        info!("indexing IDL repo {} at {}", repo, root.display());
        let repo_node = Node::new(NodeKind::Repo, repo, "", None);
        self.store.upsert_node(&repo_node)?;
        stats.nodes += 1;

        // Collect all .proto / .thrift paths first; the walk itself is cheap.
        let entries: Vec<PathBuf> = WalkBuilder::new(root)
            .build()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
            .map(|e| e.path().to_path_buf())
            .filter(|p| {
                let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("");
                ext == "proto" || ext == "thrift"
            })
            .collect();

        // Parallel parse (regex-based, no shared mutable state in parsers).
        let parsed: Vec<(PathBuf, crate::adapters::idl::protobuf::IdlFile)> = entries
            .par_iter()
            .filter_map(|p| {
                let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("");
                let result = match ext {
                    "proto" => proto_p.parse(p),
                    "thrift" => thrift_p.parse(p),
                    _ => return None,
                };
                match result {
                    Ok(i) => Some((p.clone(), i)),
                    Err(e) => {
                        warn!("skip {}: {}", p.display(), e);
                        None
                    }
                }
            })
            .collect();

        // Serial write — single SQLite connection, no concurrent writes needed.
        for (path, idl) in parsed {
            let rel = rel_path_str(path.strip_prefix(root).unwrap_or(&path));
            for svc in &idl.services {
                let svc_node = Node::new(NodeKind::IdlService, repo, &rel, Some(&svc.name));
                self.store.upsert_node(&svc_node)?;
                self.store.upsert_edge(&Edge {
                    from: repo_node.id.clone(),
                    to: svc_node.id.clone(),
                    kind: EdgeKind::Defines,
                    confidence: 1.0,
                })?;
                stats.nodes += 1;
                stats.edges += 1;

                for m in &svc.methods {
                    let qualified = format!("{}.{}", svc.name, m.name);
                    let m_node = Node::new(NodeKind::IdlMethod, repo, &rel, Some(&qualified));
                    self.store.upsert_node(&m_node)?;
                    self.store.upsert_edge(&Edge {
                        from: svc_node.id.clone(),
                        to: m_node.id.clone(),
                        kind: EdgeKind::Contains,
                        confidence: 1.0,
                    })?;
                    stats.nodes += 1;
                    stats.edges += 1;
                }
            }
        }
        Ok(())
    }

    pub fn reindex_file(&mut self, repo: &str, abs_path: &Path) -> Result<()> {
        // Find the repo's root from config — match by resolved name, not just last component,
        // so that repos with explicit `name:` fields are found correctly.
        let repo_cfg = self.config.repos.iter().find(|r| {
            let root = self.resolve_repo_path(&r.path);
            let resolved_name = r.name.clone().unwrap_or_else(|| {
                root.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default()
            });
            resolved_name == repo
        });
        let Some(rcfg) = repo_cfg.cloned() else {
            return Ok(());
        };
        let root = self.resolve_repo_path(&rcfg.path);
        let Ok(rel_path) = abs_path.strip_prefix(&root) else {
            return Ok(());
        };
        let rel = rel_path_str(rel_path);

        // ── Delete from index.db + outline.db. deps.db and search.db are
        // refreshed by the caller (incremental::update) using their own
        // path conventions (abs for deps, rel for search), so we don't
        // touch them here.
        self.store.delete_module(repo, &rel)?;
        self.outline_store.delete(repo, &rel)?;

        // If file no longer exists (deleted), cleanup is done
        if !abs_path.exists() {
            return Ok(());
        }

        // ── Re-parse via adapters (richer Declaration tree) ──────────────────
        let parse_result = match crate::adapters::parse_file(abs_path) {
            Some(pr) => pr,
            None => return Ok(()), // unsupported extension
        };

        // ── Re-write index.db ────────────────────────────────────────────────
        let repo_node = Node::new(NodeKind::Repo, repo, "", None);
        self.store.upsert_node(&repo_node)?;
        let module_node = Node::new(NodeKind::Module, repo, &rel, None);
        self.store.upsert_node(&module_node)?;
        self.store.upsert_edge(&Edge {
            from: repo_node.id.clone(),
            to: module_node.id.clone(),
            kind: EdgeKind::Contains,
            confidence: 1.0,
        })?;
        // Emit Type/Method/Function nodes from the Declaration tree (same as build_all)
        let mut dummy_stats = BuildStats::default();
        for decl in &parse_result.declarations {
            emit_decl_nodes(
                &self.store,
                &repo_node.id,
                &module_node.id,
                repo,
                &rel,
                decl,
                None,
                &mut dummy_stats,
            )?;
        }

        // ── Re-write outline.db ──────────────────────────────────────────────
        let content_hash = hash_source(&parse_result.source);
        self.outline_store
            .upsert(repo, &parse_result, &content_hash)?;

        Ok(())
    }

    fn resolve_repo_path(&self, p: &Path) -> PathBuf {
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            self.workspace_root.join(p)
        }
    }
}

// ── Declaration tree → graph nodes ────────────────────────────────────────────

/// Recursively walk a `Declaration` tree and emit `Type`, `Method`, and
/// `Function` nodes + `Contains` edges into `store`.
///
/// Mapping:
/// - Class/Struct/Interface/Record/Enum → `NodeKind::Type`; children recurse
///   with `parent_type_id` set.
/// - Method/Constructor/Destructor/Operator inside a Type → `NodeKind::Method`
/// - Function (or top-level Method) → `NodeKind::Function`
/// - Namespace → descend without emitting a node (Go package, C# namespace)
/// - Field/Property/EnumMember/… → skip (live in outline.db only)
#[allow(clippy::too_many_arguments)]
fn emit_decl_nodes(
    store: &Store,
    _repo_node_id: &str,
    module_node_id: &str,
    repo: &str,
    path: &str,
    decl: &crate::core::declaration::Declaration,
    parent_type_id: Option<&str>,
    stats: &mut BuildStats,
) -> Result<()> {
    use crate::core::declaration::DeclarationKind;

    match decl.kind {
        DeclarationKind::Class
        | DeclarationKind::Struct
        | DeclarationKind::Interface
        | DeclarationKind::Record
        | DeclarationKind::Enum => {
            let mut n = Node::new(NodeKind::Type, repo, path, Some(&decl.name));
            n.loc_start = Some(decl.start_line as u32);
            n.loc_end = Some(decl.end_line as u32);
            n.visibility = Some(decl.visibility.clone());
            n.native_kind = decl.native_kind.clone();
            n.deprecated = decl.deprecated;
            store.upsert_node(&n)?;
            let parent_id = parent_type_id.unwrap_or(module_node_id);
            store.upsert_edge(&Edge {
                from: parent_id.into(),
                to: n.id.clone(),
                kind: EdgeKind::Contains,
                confidence: 1.0,
            })?;
            stats.nodes += 1;
            stats.edges += 1;

            // Recurse into children with this type as the parent context
            for child in &decl.children {
                emit_decl_nodes(
                    store,
                    _repo_node_id,
                    module_node_id,
                    repo,
                    path,
                    child,
                    Some(&n.id),
                    stats,
                )?;
            }
        }

        DeclarationKind::Method
        | DeclarationKind::Constructor
        | DeclarationKind::Destructor
        | DeclarationKind::Operator => {
            let mut n = Node::new(NodeKind::Method, repo, path, Some(&decl.name));
            n.loc_start = Some(decl.start_line as u32);
            n.loc_end = Some(decl.end_line as u32);
            n.visibility = Some(decl.visibility.clone());
            n.deprecated = decl.deprecated;
            store.upsert_node(&n)?;
            let parent_id = parent_type_id.unwrap_or(module_node_id);
            store.upsert_edge(&Edge {
                from: parent_id.into(),
                to: n.id.clone(),
                kind: EdgeKind::Contains,
                confidence: 1.0,
            })?;
            stats.nodes += 1;
            stats.edges += 1;
        }

        DeclarationKind::Function => {
            let mut n = Node::new(NodeKind::Function, repo, path, Some(&decl.name));
            n.loc_start = Some(decl.start_line as u32);
            n.loc_end = Some(decl.end_line as u32);
            n.visibility = Some(decl.visibility.clone());
            n.deprecated = decl.deprecated;
            store.upsert_node(&n)?;
            store.upsert_edge(&Edge {
                from: module_node_id.into(),
                to: n.id.clone(),
                kind: EdgeKind::Contains,
                confidence: 1.0,
            })?;
            stats.nodes += 1;
            stats.edges += 1;
        }

        DeclarationKind::Namespace => {
            // Don't emit a node for namespaces — just descend.
            // (Go package declarations, C# namespaces)
            for child in &decl.children {
                emit_decl_nodes(
                    store,
                    _repo_node_id,
                    module_node_id,
                    repo,
                    path,
                    child,
                    parent_type_id,
                    stats,
                )?;
            }
        }

        // Field, Property, EnumMember, Indexer, Event, Delegate, Heading, CodeBlock
        // → live in outline.db only; skip in main graph.
        _ => {}
    }
    Ok(())
}

// ── Import extraction (via deps::extract) ─────────────────────────────────────

/// Extract raw import specifiers from a source file using `deps::extract`.
/// Returns an empty Vec for unsupported extensions.
fn extract_imports_for_file(abs_path: &Path) -> Vec<String> {
    use crate::deps::resolver::build::Lang;
    let ext = abs_path.extension().and_then(|s| s.to_str()).unwrap_or("");
    let lang = match ext {
        "ts" => Lang::TypeScript,
        "tsx" => Lang::Tsx,
        "js" | "jsx" | "mjs" => Lang::JavaScript,
        "py" => Lang::Python,
        "go" => Lang::Go,
        _ => return Vec::new(),
    };
    crate::deps::extract::extract(abs_path, lang)
        .into_iter()
        .map(|i| i.spec)
        .collect()
}

/// Derive a content hash using xxhash-rust (xxh3 128-bit) for change detection.
/// Returns 16 bytes. Falls back to all-zeros on empty source.
fn hash_source(src: &[u8]) -> Vec<u8> {
    use xxhash_rust::xxh3::xxh3_128;
    if src.is_empty() {
        return vec![0u8; 16];
    }
    let h = xxh3_128(src);
    h.to_le_bytes().to_vec()
}

/// Result of the embedding phase. Used by `Indexer::build_all` to decide
/// what to log (info vs warn).
enum EmbedOutcome {
    /// Successfully embedded `n` chunks across all repos.
    Done(usize),
    /// Skipped without trying — model isn't in cache and the user hasn't
    /// opted into a download. The string explains the precise cause.
    Skipped(&'static str),
    /// Tried but failed (download error, corrupt model, …). Reported as a
    /// warn but never aborts the build.
    Failed(anyhow::Error),
}

/// Look for a cached potion-code-16M model. Honours `AST_OUTLINE_MODEL_DIR`
/// (the same env var `download::cache_root` uses) so users with custom
/// caches don't need to re-download.
fn cached_model_present() -> bool {
    use crate::search::download::{model_dir, ModelInfo};
    let Ok(dir) = model_dir(&ModelInfo::potion_code_16m()) else {
        return false;
    };
    dir.join("model.safetensors").is_file()
        && dir.join("tokenizer.json").is_file()
        && dir.join("manifest.json").is_file()
}

/// Embed every chunk in `search.db` for each of the supplied repos.
///
/// Uses the dynamic [`crate::search::embedder::Embedder`] when an
/// `embedding:` block is present in the config, otherwise falls back to the
/// legacy v0.2.0-alpha potion-code-16M path for back-compat. Never panics;
/// turns I/O errors into [`EmbedOutcome::Failed`].
async fn try_embed_async(
    store: &mut crate::search::store::SearchStore,
    repos: &[(String, std::path::PathBuf)],
    cfg: Option<&crate::config::EmbeddingConfig>,
    stats: &mut BuildStats,
) -> EmbedOutcome {
    let (embedder, http_stats) = match crate::search::embedder::make_embedder(cfg) {
        Ok(Some(pair)) => pair,
        Ok(None) => {
            // No config — fall back to legacy potion behaviour.
            return legacy_try_embed_potion(store, repos);
        }
        Err(e) => return EmbedOutcome::Failed(e),
    };

    let mut total = 0usize;
    for (repo_name, _) in repos {
        let chunks = match store.list_chunks(repo_name) {
            Ok(c) => c,
            Err(e) => {
                snapshot_http_stats(http_stats.as_ref(), stats);
                return EmbedOutcome::Failed(e);
            }
        };
        // Build a flat (id, text) list, batch-encode, then upsert.
        let texts: Vec<String> = chunks.iter().map(|(_, _, _, _, c)| c.clone()).collect();
        let vectors = match embedder.encode_batch(&texts).await {
            Ok(v) => v,
            Err(e) => {
                snapshot_http_stats(http_stats.as_ref(), stats);
                return EmbedOutcome::Failed(e);
            }
        };
        if vectors.len() != chunks.len() {
            snapshot_http_stats(http_stats.as_ref(), stats);
            return EmbedOutcome::Failed(anyhow::anyhow!(
                "embedder returned {} vectors for {} chunks in repo {}",
                vectors.len(),
                chunks.len(),
                repo_name
            ));
        }
        // Bulk upsert wrapped in a single transaction. vec0 0.1.9 loses
        // writes silently when fed thousands of autocommit inserts.
        let pairs: Vec<(i64, Vec<f32>)> = chunks
            .iter()
            .zip(vectors)
            .map(|((id, _, _, _, _), v)| (*id, v))
            .collect();
        if let Err(e) = store.upsert_embeddings_batch(&pairs) {
            snapshot_http_stats(http_stats.as_ref(), stats);
            return EmbedOutcome::Failed(e);
        }
        total += chunks.len();
        info!("embedded {} chunks for repo {}", chunks.len(), repo_name);
    }
    snapshot_http_stats(http_stats.as_ref(), stats);
    EmbedOutcome::Done(total)
}

/// Copy the HTTP embedder atomic counters into `BuildStats`. No-op for
/// non-HTTP providers (their `http_stats` is `None`). Called at every exit
/// point of `try_embed_async` so the caller sees the same observed values on
/// the success and failure paths.
fn snapshot_http_stats(
    http: Option<&std::sync::Arc<crate::search::http_embedder::HttpEmbedderStats>>,
    stats: &mut BuildStats,
) {
    use std::sync::atomic::Ordering;
    if let Some(s) = http {
        stats.embed_requests = s.requests.load(Ordering::Relaxed);
        stats.embed_retries = s.retries.load(Ordering::Relaxed);
        stats.embed_input_chars = s.input_chars.load(Ordering::Relaxed);
        stats.embed_vectors = s.vectors_returned.load(Ordering::Relaxed);
    }
}

/// Legacy path: same behaviour as the v0.2.0-alpha `try_embed` — preserved
/// so `repolayer.yml` files without an `embedding:` block still work.
fn legacy_try_embed_potion(
    store: &mut crate::search::store::SearchStore,
    repos: &[(String, std::path::PathBuf)],
) -> EmbedOutcome {
    use crate::search::download::{ensure_model, ModelInfo};
    use crate::search::embed::Embedder;

    let opt_in_download = std::env::var("REPOLAYER_DOWNLOAD")
        .ok()
        .filter(|v| !v.is_empty() && v != "0" && !v.eq_ignore_ascii_case("false"))
        .is_some();
    let no_download = std::env::var_os("REPOLAYER_NO_DOWNLOAD").is_some();

    let cached = cached_model_present();
    if no_download && !cached {
        return EmbedOutcome::Skipped("REPOLAYER_NO_DOWNLOAD is set and model isn't cached");
    }
    if !opt_in_download && !cached {
        return EmbedOutcome::Skipped("model not cached and REPOLAYER_DOWNLOAD not set");
    }

    let info = ModelInfo::potion_code_16m();
    let model_dir = match ensure_model(&info) {
        Ok(d) => d,
        Err(e) => return EmbedOutcome::Failed(anyhow::anyhow!("download failed: {}", e)),
    };
    let embedder = match Embedder::open(&model_dir) {
        Ok(e) => e,
        Err(e) => {
            return EmbedOutcome::Failed(anyhow::anyhow!(
                "loading embedder from {}: {}",
                model_dir.display(),
                e
            ))
        }
    };

    let mut total = 0usize;
    for (repo_name, _) in repos {
        let chunks = match store.list_chunks(repo_name) {
            Ok(c) => c,
            Err(e) => return EmbedOutcome::Failed(e),
        };
        let pairs: Vec<(i64, Vec<f32>)> = chunks
            .iter()
            .map(|(id, _, _, _, content)| (*id, embedder.encode_one(content).to_vec()))
            .collect();
        if let Err(e) = store.upsert_embeddings_batch(&pairs) {
            return EmbedOutcome::Failed(e);
        }
        total += chunks.len();
    }
    EmbedOutcome::Done(total)
}

fn build_llm_provider(
    cfg: &crate::config::LlmConfig,
) -> Result<std::sync::Arc<dyn crate::llm::LlmProvider>> {
    let api_key = std::env::var(&cfg.api_key_env)
        .with_context(|| format!("env var {} not set", cfg.api_key_env))?;
    match cfg.provider.as_str() {
        "anthropic" => Ok(std::sync::Arc::new(
            crate::llm::anthropic::AnthropicProvider::new(&api_key, "https://api.anthropic.com"),
        )),
        "deepseek" => Ok(std::sync::Arc::new(
            crate::llm::deepseek::DeepSeekProvider::new(&api_key, "https://api.deepseek.com"),
        )),
        other => anyhow::bail!("unknown LLM provider: {}", other),
    }
}

fn resolve_import(repo_root: &Path, from_file: &Path, spec: &str) -> Option<PathBuf> {
    if !spec.starts_with('.') {
        return None;
    }
    let dir = from_file.parent()?;
    let candidate = dir.join(spec);
    for ext in ["ts", "tsx", "js", "jsx", "mjs"] {
        let mut with_ext_os = candidate.clone().into_os_string();
        with_ext_os.push(".");
        with_ext_os.push(ext);
        let with_ext = PathBuf::from(with_ext_os);
        if with_ext.exists() && with_ext.starts_with(repo_root) {
            return Some(with_ext);
        }
    }
    let index = candidate.join("index.ts");
    if index.exists() && index.starts_with(repo_root) {
        return Some(index);
    }
    None
}

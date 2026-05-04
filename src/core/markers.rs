//! Marker post-processing: derives `native_kind`, `modifiers`, `deprecated`
//! fields on Declarations from the language-native conventions captured
//! by adapters in `attrs`/`signature`. Adopted from aeroxy/ast-outline
//! `src/core.rs` (lines 13-186 of the marker section).
//!
//! Each adapter emits a Declaration with the basics (kind, signature,
//! attrs, docs, visibility). The richer presentation fields —
//! `native_kind`, `modifiers`, `deprecated` — are derived from those
//! basics in one centralised pass keyed off the language string. That
//! keeps adapters dumb and fast (one tree walk only) and lets us add a
//! new marker by editing one file.

use crate::core::declaration::{Declaration, DeclarationKind};

/// Walk every declaration tree and fill in `native_kind`, `modifiers`,
/// and `deprecated` based on the language conventions. Idempotent —
/// values an adapter already set are preserved.
pub fn populate_markers(decls: &mut [Declaration], language: &str) {
    for d in decls.iter_mut() {
        _populate_one(d, language);
        if !d.children.is_empty() {
            populate_markers(&mut d.children, language);
        }
    }
}

fn _populate_one(d: &mut Declaration, lang: &str) {
    if d.native_kind.is_none() {
        d.native_kind = _native_kind(d, lang);
    }
    if d.modifiers.is_empty() {
        d.modifiers = _modifiers(d, lang);
    }
    if !d.deprecated {
        d.deprecated = _deprecated(d, lang);
    }
}

fn _native_kind(d: &Declaration, lang: &str) -> Option<String> {
    let sig = d.signature.as_str();
    // Skip the visibility prefix when looking for a leading keyword.
    let starts_with_kw = |kw: &str| -> bool {
        sig.split_whitespace()
            .find(|t| !matches!(*t, "pub" | "private" | "public" | "internal" | "protected"))
            .map(|t| t == kw)
            .unwrap_or(false)
    };
    let contains_kw_pair = |first: &str, second: &str| -> bool {
        let toks: Vec<&str> = sig.split_whitespace().collect();
        toks.windows(2).any(|w| w[0] == first && w[1] == second)
    };

    match lang {
        "rust" => {
            if matches!(d.kind, DeclarationKind::Interface) && starts_with_kw("trait") {
                Some("trait".to_string())
            } else {
                None
            }
        }
        "scala" => {
            if contains_kw_pair("case", "class") {
                Some("case class".to_string())
            } else if contains_kw_pair("case", "object") {
                Some("case object".to_string())
            } else if starts_with_kw("object") {
                Some("object".to_string())
            } else if starts_with_kw("trait") {
                Some("trait".to_string())
            } else {
                None
            }
        }
        "kotlin" => {
            if contains_kw_pair("data", "class") {
                Some("data class".to_string())
            } else if contains_kw_pair("enum", "class") {
                Some("enum class".to_string())
            } else if contains_kw_pair("sealed", "class") {
                Some("sealed class".to_string())
            } else if contains_kw_pair("companion", "object") {
                Some("companion object".to_string())
            } else if starts_with_kw("object") {
                Some("object".to_string())
            } else {
                None
            }
        }
        "java" => {
            if starts_with_kw("record") {
                Some("record".to_string())
            } else if starts_with_kw("enum") {
                Some("enum".to_string())
            } else if starts_with_kw("interface") {
                Some("interface".to_string())
            } else {
                None
            }
        }
        "csharp" => {
            if contains_kw_pair("record", "struct") {
                Some("record struct".to_string())
            } else if starts_with_kw("record") {
                Some("record".to_string())
            } else {
                None
            }
        }
        _ => None,
    }
}

fn _modifiers(d: &Declaration, lang: &str) -> Vec<String> {
    let want: &[&str] = match lang {
        "rust" => &["async", "unsafe", "const", "extern"],
        "python" => &["async"],
        "typescript" => &["async", "static", "abstract", "readonly", "override"],
        "java" => &["static", "abstract", "final", "synchronized", "default", "native"],
        "kotlin" => &[
            "suspend", "open", "inner", "value", "inline", "infix", "tailrec", "operator",
            "abstract", "override", "sealed", "final",
        ],
        "scala" => &["sealed", "final", "abstract", "implicit", "inline", "lazy", "override"],
        "csharp" => &["partial", "sealed", "static", "abstract", "virtual", "override", "async"],
        _ => &[],
    };
    if want.is_empty() {
        return Vec::new();
    }
    // Stop scanning once we hit the actual declaration keyword — anything
    // beyond is the name/parameter list, not a modifier.
    let stop_words: &[&str] = match lang {
        "rust" => &["fn", "trait", "struct", "enum", "impl", "type", "mod"],
        "python" => &["def", "class"],
        "typescript" => &[
            "function", "class", "interface", "enum", "type", "const", "let", "var",
        ],
        "java" => &["class", "interface", "enum", "record", "void"],
        "kotlin" => &["fun", "class", "interface", "object", "enum", "val", "var"],
        "scala" => &["def", "class", "trait", "object", "val", "var", "type"],
        "csharp" => &["class", "interface", "struct", "record", "enum", "void"],
        _ => &[],
    };

    let mut out = Vec::new();
    for tok in d.signature.split_whitespace() {
        if stop_words.contains(&tok) {
            break;
        }
        if want.contains(&tok) {
            out.push(tok.to_string());
        }
    }

    // Python decorators land in `attrs`, not the signature line.
    if lang == "python" {
        for a in &d.attrs {
            let trimmed = a.trim_start_matches('@');
            let name = trimmed.split('(').next().unwrap_or(trimmed).trim();
            match name {
                "classmethod" => out.push("classmethod".to_string()),
                "staticmethod" => out.push("static".to_string()),
                "abstractmethod" => out.push("abstract".to_string()),
                "property" => out.push("property".to_string()),
                _ => {}
            }
        }
    }

    out
}

fn _deprecated(d: &Declaration, lang: &str) -> bool {
    let attr_has = |needle: &str| d.attrs.iter().any(|a| a.contains(needle));
    let doc_has = |needle: &str| d.docs.iter().any(|x| x.contains(needle));
    match lang {
        "rust" => attr_has("#[deprecated") || attr_has("#[ deprecated"),
        "python" => {
            attr_has("@deprecated")
                || attr_has("@typing.deprecated")
                || attr_has("@warnings.deprecated")
        }
        "typescript" => doc_has("@deprecated") || attr_has("@deprecated"),
        "java" | "kotlin" => attr_has("@Deprecated"),
        "scala" => attr_has("@deprecated"),
        "csharp" => attr_has("[Obsolete") || attr_has("[ Obsolete"),
        // Go convention: a `Deprecated:` paragraph in the doc comment.
        "go" => doc_has("Deprecated:"),
        _ => false,
    }
}

use crate::parser::treesitter::{extract_named_symbol, node_text, parse_with};
use crate::parser::{ExportedSymbol, ParsedFile, Parser, SymbolKind};
use anyhow::{Context, Result};
use std::path::Path;

pub struct TypeScriptParser;

impl TypeScriptParser {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TypeScriptParser {
    fn default() -> Self {
        Self::new()
    }
}

impl Parser for TypeScriptParser {
    fn parse_file(&self, path: &Path) -> Result<ParsedFile> {
        let source =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let lang: tree_sitter::Language =
            if path.extension().and_then(|s| s.to_str()) == Some("tsx") {
                tree_sitter_typescript::LANGUAGE_TSX.into()
            } else if path.extension().and_then(|s| s.to_str()) == Some("ts") {
                tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
            } else {
                tree_sitter_javascript::LANGUAGE.into()
            };

        let tree = parse_with(lang, &source).context("ts parse failed")?;
        let root = tree.root_node();
        let bytes = source.as_bytes();

        let mut symbols = Vec::new();
        let mut imports = Vec::new();

        let mut cursor = root.walk();
        for child in root.named_children(&mut cursor) {
            match child.kind() {
                "import_statement" => {
                    if let Some(src) = child.child_by_field_name("source") {
                        let raw = node_text(src, bytes);
                        let trimmed = raw.trim_matches(|c| c == '"' || c == '\'' || c == '`');
                        imports.push(trimmed.to_string());
                    }
                }
                "export_statement" => {
                    extract_export(child, bytes, &mut symbols);
                }
                _ => {}
            }
        }

        Ok(ParsedFile {
            path: path.to_string_lossy().to_string(),
            symbols,
            imports,
        })
    }
}

fn extract_export(node: tree_sitter::Node, bytes: &[u8], out: &mut Vec<ExportedSymbol>) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "function_declaration" => {
                if let Some(sym) =
                    extract_named_symbol(child, child, bytes, SymbolKind::Function, |_| true)
                {
                    out.push(sym);
                }
            }
            "class_declaration" => {
                if let Some(sym) =
                    extract_named_symbol(child, child, bytes, SymbolKind::Class, |_| true)
                {
                    out.push(sym);
                }
            }
            "interface_declaration" => {
                if let Some(sym) =
                    extract_named_symbol(child, child, bytes, SymbolKind::Interface, |_| true)
                {
                    out.push(sym);
                }
            }
            "type_alias_declaration" => {
                if let Some(sym) =
                    extract_named_symbol(child, child, bytes, SymbolKind::TypeAlias, |_| true)
                {
                    out.push(sym);
                }
            }
            "lexical_declaration" | "variable_declaration" => {
                push_variable_declarators(child, bytes, out);
            }
            _ => {}
        }
    }
}

fn push_variable_declarators(
    decl_node: tree_sitter::Node,
    bytes: &[u8],
    out: &mut Vec<ExportedSymbol>,
) {
    let mut cursor = decl_node.walk();
    for child in decl_node.named_children(&mut cursor) {
        if child.kind() != "variable_declarator" {
            continue;
        }
        let Some(name_node) = child.child_by_field_name("name") else {
            continue;
        };
        // TODO: support destructuring export by extracting individual identifiers
        // from object_pattern / array_pattern. For MVP we skip them entirely.
        if matches!(name_node.kind(), "object_pattern" | "array_pattern") {
            continue;
        }
        let name = node_text(name_node, bytes).to_string();
        if name.is_empty() {
            continue;
        }
        let start = child.start_position().row as u32 + 1;
        let end = child.end_position().row as u32 + 1;
        out.push(ExportedSymbol {
            name,
            kind: SymbolKind::Const,
            loc_start: start,
            loc_end: end,
        });
    }
}

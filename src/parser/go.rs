use crate::parser::treesitter::{extract_named_symbol, node_text, parse_with};
use crate::parser::{ParsedFile, Parser, SymbolKind};
use anyhow::{Context, Result};
use std::path::Path;

pub struct GoParser;

impl GoParser {
    pub fn new() -> Self {
        Self
    }
}

impl Default for GoParser {
    fn default() -> Self {
        Self::new()
    }
}

impl Parser for GoParser {
    fn parse_file(&self, path: &Path) -> Result<ParsedFile> {
        let source =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let lang: tree_sitter::Language = tree_sitter_go::LANGUAGE.into();
        let tree = parse_with(lang, &source).context("go parse failed")?;
        let root = tree.root_node();
        let bytes = source.as_bytes();

        let mut symbols = Vec::new();
        let mut imports = Vec::new();
        let mut cursor = root.walk();

        let exported = |n: &str| n.chars().next().map(|c| c.is_uppercase()).unwrap_or(false);

        for child in root.named_children(&mut cursor) {
            match child.kind() {
                "import_declaration" => {
                    collect_imports(child, bytes, &mut imports);
                }
                "function_declaration" => {
                    if let Some(sym) =
                        extract_named_symbol(child, child, bytes, SymbolKind::Function, exported)
                    {
                        symbols.push(sym);
                    }
                }
                "method_declaration" => {
                    if let Some(sym) =
                        extract_named_symbol(child, child, bytes, SymbolKind::Function, exported)
                    {
                        symbols.push(sym);
                    }
                }
                "type_declaration" => {
                    let mut c = child.walk();
                    for spec in child.named_children(&mut c) {
                        if matches!(spec.kind(), "type_spec" | "type_alias") {
                            if let Some(sym) =
                                extract_named_symbol(spec, spec, bytes, SymbolKind::Class, exported)
                            {
                                symbols.push(sym);
                            }
                        }
                    }
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

fn collect_imports(decl: tree_sitter::Node, bytes: &[u8], out: &mut Vec<String>) {
    let mut cursor = decl.walk();
    for child in decl.named_children(&mut cursor) {
        match child.kind() {
            "import_spec" => {
                if let Some(p) = child.child_by_field_name("path") {
                    let raw = node_text(p, bytes);
                    out.push(raw.trim_matches('"').to_string());
                }
            }
            "import_spec_list" => {
                let mut c = child.walk();
                for spec in child.named_children(&mut c) {
                    if spec.kind() == "import_spec" {
                        if let Some(p) = spec.child_by_field_name("path") {
                            let raw = node_text(p, bytes);
                            out.push(raw.trim_matches('"').to_string());
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

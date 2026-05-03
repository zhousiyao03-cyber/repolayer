use crate::parser::treesitter::{extract_named_symbol, node_text, parse_with};
use crate::parser::{ParsedFile, Parser, SymbolKind};
use anyhow::{Context, Result};
use std::path::Path;

pub struct PythonParser;

impl PythonParser {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PythonParser {
    fn default() -> Self {
        Self::new()
    }
}

impl Parser for PythonParser {
    fn parse_file(&self, path: &Path) -> Result<ParsedFile> {
        let source =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let lang: tree_sitter::Language = tree_sitter_python::LANGUAGE.into();
        let tree = parse_with(lang, &source).context("python parse failed")?;
        let root = tree.root_node();
        let bytes = source.as_bytes();

        let mut symbols = Vec::new();
        let mut imports = Vec::new();

        let mut cursor = root.walk();
        for child in root.named_children(&mut cursor) {
            match child.kind() {
                "import_statement" => {
                    let mut c = child.walk();
                    for n in child.named_children(&mut c) {
                        if n.kind() == "dotted_name" {
                            imports.push(node_text(n, bytes).to_string());
                        }
                    }
                }
                "import_from_statement" => {
                    // TODO(task-11): relative imports are captured as raw "." / ".." / "..parent"
                    // text. Task 11 (cross-repo linker) will need to resolve them against the
                    // file's package path.
                    if let Some(m) = child.child_by_field_name("module_name") {
                        imports.push(node_text(m, bytes).to_string());
                    }
                }
                "function_definition" => {
                    if let Some(sym) =
                        extract_named_symbol(child, child, bytes, SymbolKind::Function, |n| {
                            !n.starts_with('_')
                        })
                    {
                        symbols.push(sym);
                    }
                }
                "class_definition" => {
                    if let Some(sym) =
                        extract_named_symbol(child, child, bytes, SymbolKind::Class, |n| {
                            !n.starts_with('_')
                        })
                    {
                        symbols.push(sym);
                    }
                }
                "decorated_definition" => {
                    // The outer `decorated_definition` wraps decorator(s) + the inner
                    // function_definition / class_definition. We use the outer node for
                    // loc so that the location includes the decorator lines.
                    let mut inner_cursor = child.walk();
                    for inner in child.named_children(&mut inner_cursor) {
                        let kind = match inner.kind() {
                            "function_definition" => SymbolKind::Function,
                            "class_definition" => SymbolKind::Class,
                            _ => continue,
                        };
                        if let Some(sym) =
                            extract_named_symbol(inner, child, bytes, kind, |n| !n.starts_with('_'))
                        {
                            symbols.push(sym);
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

use crate::parser::{ExportedSymbol, SymbolKind};
use tree_sitter::{Language, Node, Parser as TsParser, Tree};

pub fn parse_with(language: Language, source: &str) -> Option<Tree> {
    let mut parser = TsParser::new();
    parser.set_language(&language).ok()?;
    parser.parse(source, None)
}

pub fn node_text<'a>(node: Node<'a>, source: &'a [u8]) -> &'a str {
    std::str::from_utf8(&source[node.byte_range()]).unwrap_or("")
}

/// Extract a named symbol from a node that has a `name` field child.
/// `name_owner` provides the `name` field (e.g. function_definition / class_definition).
/// `kind_node` provides the loc range — this may differ from `name_owner` when the
/// symbol is wrapped in e.g. `decorated_definition`.
/// Returns None if no name found, or if `filter` rejects the name.
pub fn extract_named_symbol(
    name_owner: Node,
    kind_node: Node,
    bytes: &[u8],
    kind: SymbolKind,
    filter: impl Fn(&str) -> bool,
) -> Option<ExportedSymbol> {
    let name_node = name_owner.child_by_field_name("name")?;
    let name = node_text(name_node, bytes);
    if !filter(name) {
        return None;
    }
    Some(ExportedSymbol {
        name: name.to_string(),
        kind,
        loc_start: kind_node.start_position().row as u32 + 1,
        loc_end: kind_node.end_position().row as u32 + 1,
    })
}

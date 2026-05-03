use tree_sitter::{Language, Node, Parser as TsParser, Tree};

pub fn parse_with(language: Language, source: &str) -> Option<Tree> {
    let mut parser = TsParser::new();
    parser.set_language(&language).ok()?;
    parser.parse(source, None)
}

pub fn node_text<'a>(node: Node<'a>, source: &'a [u8]) -> &'a str {
    std::str::from_utf8(&source[node.byte_range()]).unwrap_or("")
}

//! Objective-C adapter built on bare tree-sitter (`tree-sitter-objc`).
//!
//! Objective-C is not part of `ast-grep-language`, so — like the IDL parsers
//! and the markdown adapter — this one walks tree-sitter nodes directly and
//! does **not** implement [`crate::adapters::base::LanguageAdapter`].

use crate::core::declaration::{Declaration, DeclarationKind, ParseResult};
use std::path::Path;

pub fn parse_objc(path: &Path, source: &[u8]) -> ParseResult {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_objc::LANGUAGE.into())
        .expect("objc grammar");
    let tree = parser.parse(source, None);

    let mut decls = Vec::new();
    if let Some(tree) = &tree {
        _walk_top(tree.root_node(), source, &mut decls);
    }

    ParseResult {
        path: path.to_path_buf(),
        language: "objc",
        source: source.to_vec(),
        line_count: source.iter().filter(|&&b| b == b'\n').count() + 1,
        declarations: decls,
        error_count: 0,
    }
}

fn _walk_top(node: tree_sitter::Node, src: &[u8], out: &mut Vec<Declaration>) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            // @interface Foo : NSObject  /  @interface Foo (Category)
            // @implementation Foo
            "class_interface" | "class_implementation" => {
                if let Some(d) = _class_to_decl(child, src) {
                    out.push(d);
                }
            }
            // Some grammar versions expose category interfaces as their own kind.
            "category_interface" | "category_implementation" => {
                if let Some(d) = _class_to_decl(child, src) {
                    out.push(d);
                }
            }
            "protocol_declaration" => {
                if let Some(d) = _protocol_to_decl(child, src) {
                    out.push(d);
                }
            }
            _ => {}
        }
    }
}

fn _class_to_decl(node: tree_sitter::Node, src: &[u8]) -> Option<Declaration> {
    // The first `identifier` child is the class name; `superclass` field (when
    // present) is the base class; a `category` field marks a category.
    let name = _first_identifier(node, src)?;
    let mut full_name = name.clone();
    if let Some(cat) = node.child_by_field_name("category").map(|n| _text(n, src)) {
        full_name = format!("{}({})", name, cat);
    }

    let mut bases = Vec::new();
    if let Some(sup) = node.child_by_field_name("superclass") {
        bases.push(_text(sup, src));
    }

    let mut children = Vec::new();
    _collect_members(node, src, &mut children);

    let signature = _first_line(node, src);
    Some(Declaration {
        kind: DeclarationKind::Class,
        name: full_name,
        signature,
        bases,
        visibility: String::new(),
        start_line: node.start_position().row + 1,
        end_line: _end_line(node),
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
        doc_start_byte: node.start_byte(),
        children,
        ..Default::default()
    })
}

fn _protocol_to_decl(node: tree_sitter::Node, src: &[u8]) -> Option<Declaration> {
    let name = _first_identifier(node, src)?;
    let mut children = Vec::new();
    _collect_members(node, src, &mut children);

    let signature = _first_line(node, src);
    Some(Declaration {
        kind: DeclarationKind::Interface,
        name,
        signature,
        visibility: String::new(),
        start_line: node.start_position().row + 1,
        end_line: _end_line(node),
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
        doc_start_byte: node.start_byte(),
        children,
        ..Default::default()
    })
}

/// Walk the body of an interface/implementation/protocol, pulling out methods
/// and properties. `method_definition` nodes are nested under
/// `implementation_definition`, so we recurse one level for those.
fn _collect_members(node: tree_sitter::Node, src: &[u8], out: &mut Vec<Declaration>) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "method_declaration" | "method_definition" => {
                out.push(_method_to_decl(child, src));
            }
            "property_declaration" => {
                if let Some(d) = _property_to_decl(child, src) {
                    out.push(d);
                }
            }
            // @implementation wraps each method in an `implementation_definition`.
            "implementation_definition" => {
                _collect_members(child, src, out);
            }
            _ => {}
        }
    }
}

fn _method_to_decl(node: tree_sitter::Node, src: &[u8]) -> Declaration {
    let name = _selector_name(node, src);
    // Signature: text up to the method body (compound_statement), if present.
    let body_start = {
        let mut cursor = node.walk();
        let body = node
            .named_children(&mut cursor)
            .find(|c| c.kind() == "compound_statement");
        body.map(|c| c.start_byte())
    };
    let end = body_start.unwrap_or(node.end_byte());
    let signature = _collapse(&src[node.start_byte()..end])
        .trim_end_matches('{')
        .trim()
        .trim_end_matches(';')
        .trim()
        .to_string();

    Declaration {
        kind: DeclarationKind::Method,
        name,
        signature,
        visibility: String::new(),
        start_line: node.start_position().row + 1,
        end_line: _end_line(node),
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
        doc_start_byte: node.start_byte(),
        ..Default::default()
    }
}

/// Reconstruct an Objective-C selector name, e.g. `doThing:withCount:`.
/// The selector keywords are the direct `identifier` children of the method
/// node; each one that is immediately followed by a `method_parameter` takes a
/// trailing colon.
fn _selector_name(node: tree_sitter::Node, src: &[u8]) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut pending_keyword: Option<String> = None;
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "identifier" => {
                if let Some(k) = pending_keyword.take() {
                    parts.push(k);
                }
                pending_keyword = Some(_text(child, src));
            }
            "method_parameter" => {
                if let Some(k) = pending_keyword.take() {
                    parts.push(format!("{}:", k));
                }
            }
            _ => {}
        }
    }
    if let Some(k) = pending_keyword.take() {
        parts.push(k);
    }
    if parts.is_empty() {
        "?".to_string()
    } else {
        parts.join("")
    }
}

fn _property_to_decl(node: tree_sitter::Node, src: &[u8]) -> Option<Declaration> {
    // The property name is the declarator identifier, possibly behind a pointer.
    let name = _find_declarator_identifier(node, src)?;
    let signature = _collapse(&src[node.byte_range()])
        .trim_end_matches(';')
        .trim()
        .to_string();
    Some(Declaration {
        kind: DeclarationKind::Property,
        name,
        signature,
        visibility: String::new(),
        start_line: node.start_position().row + 1,
        end_line: _end_line(node),
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
        doc_start_byte: node.start_byte(),
        ..Default::default()
    })
}

/// Recursively look for the innermost `identifier` under a declarator chain
/// (`struct_declarator` → `pointer_declarator` → … → `identifier`).
fn _find_declarator_identifier(node: tree_sitter::Node, src: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "identifier" => return Some(_text(child, src)),
            "struct_declaration"
            | "struct_declarator"
            | "pointer_declarator"
            | "function_declarator"
            | "array_declarator" => {
                if let Some(n) = _find_declarator_identifier(child, src) {
                    return Some(n);
                }
            }
            _ => {}
        }
    }
    None
}

fn _first_identifier(node: tree_sitter::Node, src: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    let found = node
        .named_children(&mut cursor)
        .find(|c| c.kind() == "identifier");
    found.map(|c| _text(c, src))
}

fn _text(node: tree_sitter::Node, src: &[u8]) -> String {
    String::from_utf8_lossy(&src[node.byte_range()]).into_owned()
}

fn _first_line(node: tree_sitter::Node, src: &[u8]) -> String {
    let s = String::from_utf8_lossy(&src[node.byte_range()]);
    s.lines().next().unwrap_or("").trim().to_string()
}

fn _collapse(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn _end_line(node: tree_sitter::Node) -> usize {
    let end_pos = node.end_position();
    let mut end_row = end_pos.row;
    if end_pos.column == 0 && end_row > node.start_position().row {
        end_row -= 1;
    }
    end_row + 1
}

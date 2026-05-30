use super::base::{collapse_ws, count_parse_errors, LanguageAdapter};
use crate::core::declaration::{Declaration, DeclarationKind, ParseResult};
use ast_grep_core::{Doc, Node};
use std::path::Path;

pub struct SwiftAdapter;

impl LanguageAdapter for SwiftAdapter {
    fn language_name(&self) -> &'static str {
        "swift"
    }

    fn parse<'a, D: Doc>(&self, path: &Path, source: &[u8], root: Node<'a, D>) -> ParseResult {
        let mut decls = Vec::new();
        _walk_top(&root, source, &mut decls);
        ParseResult {
            path: path.to_path_buf(),
            language: self.language_name(),
            source: source.to_vec(),
            line_count: source.iter().filter(|&&b| b == b'\n').count() + 1,
            declarations: decls,
            error_count: count_parse_errors(root.clone()),
        }
    }
}

fn _walk_top<'a, D: Doc>(node: &Node<'a, D>, src: &[u8], out: &mut Vec<Declaration>) {
    for child in node.children() {
        if !child.is_named() {
            continue;
        }
        match child.kind().as_ref() {
            // `class_declaration` covers class / struct / enum / extension in
            // the Swift grammar; the leading keyword and body node disambiguate.
            "class_declaration" => {
                if let Some(d) = _type_decl_to_decl(&child, src) {
                    out.push(d);
                }
            }
            "protocol_declaration" => {
                if let Some(d) = _protocol_to_decl(&child, src) {
                    out.push(d);
                }
            }
            "function_declaration" => {
                // Top-level function → Function.
                out.push(_function_to_decl(&child, src, DeclarationKind::Function));
            }
            "property_declaration" => {
                if let Some(d) = _property_to_decl(&child, src) {
                    out.push(d);
                }
            }
            _ => {}
        }
    }
}

/// Determine the declaration kind from the first keyword token of a
/// `class_declaration` node (which the grammar reuses for class/struct/enum/
/// extension).
fn _type_keyword<'a, D: Doc>(node: &Node<'a, D>) -> &'static str {
    for c in node.children() {
        if c.is_named() {
            continue;
        }
        match c.kind().as_ref() {
            "class" => return "class",
            "struct" => return "struct",
            "enum" => return "enum",
            "extension" => return "extension",
            "actor" => return "actor",
            _ => {}
        }
    }
    "class"
}

fn _type_decl_to_decl<'a, D: Doc>(node: &Node<'a, D>, src: &[u8]) -> Option<Declaration> {
    let keyword = _type_keyword(node);
    let kind = match keyword {
        "struct" => DeclarationKind::Struct,
        "enum" => DeclarationKind::Enum,
        // `extension` re-opens an existing type; surface it as a Class so it
        // shows up in outlines, carrying its members.
        _ => DeclarationKind::Class,
    };

    let name = _first_type_identifier(node)?;
    let bases = _inheritance_bases(node);
    let signature = _slice_until_body(node, src);

    let mut children = Vec::new();
    if let Some(body) = node.children().find(|c| {
        matches!(
            c.kind().as_ref(),
            "class_body" | "enum_class_body" | "protocol_body"
        )
    }) {
        _walk_type_body(&body, src, &mut children);
    }

    let range = node.range();
    Some(Declaration {
        kind,
        name,
        signature,
        bases,
        visibility: String::new(),
        start_line: node.start_pos().line() + 1,
        end_line: node.end_pos().line() + 1,
        start_byte: range.start,
        end_byte: range.end,
        doc_start_byte: range.start,
        children,
        ..Default::default()
    })
}

fn _protocol_to_decl<'a, D: Doc>(node: &Node<'a, D>, src: &[u8]) -> Option<Declaration> {
    let name = _first_type_identifier(node)?;
    let bases = _inheritance_bases(node);
    let signature = _slice_until_body(node, src);

    let mut children = Vec::new();
    if let Some(body) = node.children().find(|c| c.kind() == "protocol_body") {
        _walk_type_body(&body, src, &mut children);
    }

    let range = node.range();
    Some(Declaration {
        kind: DeclarationKind::Interface,
        name,
        signature,
        bases,
        visibility: String::new(),
        start_line: node.start_pos().line() + 1,
        end_line: node.end_pos().line() + 1,
        start_byte: range.start,
        end_byte: range.end,
        doc_start_byte: range.start,
        children,
        ..Default::default()
    })
}

fn _walk_type_body<'a, D: Doc>(body: &Node<'a, D>, src: &[u8], out: &mut Vec<Declaration>) {
    for c in body.children() {
        if !c.is_named() {
            continue;
        }
        match c.kind().as_ref() {
            // Functions inside a type become Methods.
            "function_declaration" | "protocol_function_declaration" => {
                out.push(_function_to_decl(&c, src, DeclarationKind::Method));
            }
            "property_declaration" => {
                if let Some(d) = _property_to_decl(&c, src) {
                    out.push(d);
                }
            }
            "enum_entry" => {
                out.extend(_enum_entry_to_decls(&c, src));
            }
            // Nested types.
            "class_declaration" => {
                if let Some(d) = _type_decl_to_decl(&c, src) {
                    out.push(d);
                }
            }
            "protocol_declaration" => {
                if let Some(d) = _protocol_to_decl(&c, src) {
                    out.push(d);
                }
            }
            _ => {}
        }
    }
}

fn _function_to_decl<'a, D: Doc>(
    node: &Node<'a, D>,
    src: &[u8],
    kind: DeclarationKind,
) -> Declaration {
    let name = node
        .children()
        .find(|c| c.kind() == "simple_identifier")
        .map(|n| n.text().into_owned())
        .unwrap_or_else(|| "?".to_string());

    // Signature: everything up to the function body (if any).
    let body = node.children().find(|c| c.kind() == "function_body");
    let end = body.map(|b| b.range().start).unwrap_or(node.range().end);
    let signature = collapse_ws(&String::from_utf8_lossy(&src[node.range().start..end]))
        .trim_end_matches('{')
        .trim()
        .to_string();

    let range = node.range();
    Declaration {
        kind,
        name,
        signature,
        visibility: String::new(),
        start_line: node.start_pos().line() + 1,
        end_line: node.end_pos().line() + 1,
        start_byte: range.start,
        end_byte: range.end,
        doc_start_byte: range.start,
        ..Default::default()
    }
}

fn _property_to_decl<'a, D: Doc>(node: &Node<'a, D>, src: &[u8]) -> Option<Declaration> {
    // The bound name lives under a `pattern` → `simple_identifier`.
    let name = node
        .children()
        .find(|c| c.kind() == "pattern")
        .and_then(|p| {
            p.children()
                .find(|c| c.kind() == "simple_identifier")
                .map(|n| n.text().into_owned())
        })?;

    let signature = collapse_ws(&String::from_utf8_lossy(&src[node.range()]));
    let range = node.range();
    Some(Declaration {
        kind: DeclarationKind::Property,
        name,
        signature,
        visibility: String::new(),
        start_line: node.start_pos().line() + 1,
        end_line: node.end_pos().line() + 1,
        start_byte: range.start,
        end_byte: range.end,
        doc_start_byte: range.start,
        ..Default::default()
    })
}

fn _enum_entry_to_decls<'a, D: Doc>(node: &Node<'a, D>, _src: &[u8]) -> Vec<Declaration> {
    // `case red, green` produces one `enum_entry` with several
    // `simple_identifier` children — one EnumMember each.
    let mut out = Vec::new();
    let range = node.range();
    for c in node.children() {
        if c.kind() != "simple_identifier" {
            continue;
        }
        let name = c.text().into_owned();
        out.push(Declaration {
            kind: DeclarationKind::EnumMember,
            name: name.clone(),
            signature: format!("case {}", name),
            visibility: String::new(),
            start_line: node.start_pos().line() + 1,
            end_line: node.end_pos().line() + 1,
            start_byte: range.start,
            end_byte: range.end,
            doc_start_byte: range.start,
            ..Default::default()
        });
    }
    out
}

fn _first_type_identifier<'a, D: Doc>(node: &Node<'a, D>) -> Option<String> {
    node.children()
        .find(|c| c.kind() == "type_identifier")
        .map(|n| n.text().into_owned())
}

fn _inheritance_bases<'a, D: Doc>(node: &Node<'a, D>) -> Vec<String> {
    let mut bases = Vec::new();
    for c in node.children() {
        if c.kind() == "inheritance_specifier" {
            let t = collapse_ws(&c.text());
            if !t.is_empty() {
                bases.push(t);
            }
        }
    }
    bases
}

/// Slice the declaration text from its start up to the opening body brace,
/// yielding a clean one-line signature.
fn _slice_until_body<'a, D: Doc>(node: &Node<'a, D>, src: &[u8]) -> String {
    let cut = node
        .children()
        .find(|c| {
            matches!(
                c.kind().as_ref(),
                "class_body" | "enum_class_body" | "protocol_body"
            )
        })
        .map(|b| b.range().start)
        .unwrap_or(node.range().end);
    collapse_ws(&String::from_utf8_lossy(&src[node.range().start..cut]))
        .trim_end_matches('{')
        .trim()
        .to_string()
}

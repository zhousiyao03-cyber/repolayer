use super::base::{collapse_ws, count_parse_errors, field_text, LanguageAdapter};
use crate::core::declaration::{Declaration, DeclarationKind, ParseResult};
use ast_grep_core::{Doc, Node};
use std::path::Path;

pub struct PythonAdapter;

impl LanguageAdapter for PythonAdapter {
    fn language_name(&self) -> &'static str {
        "python"
    }

    fn parse<'a, D: Doc>(&self, path: &Path, source: &[u8], root: Node<'a, D>) -> ParseResult {
        let mut decls = Vec::new();
        _walk_module(&root, source, &mut decls);
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

fn _walk_module<'a, D: Doc>(node: &Node<'a, D>, src: &[u8], out: &mut Vec<Declaration>) {
    for child in node.children() {
        if !child.is_named() {
            continue;
        }
        if let Some(decl) = _node_to_decl(&child, src, false) {
            out.push(decl);
        }
    }
}

fn _walk_class_body<'a, D: Doc>(block: &Node<'a, D>, src: &[u8]) -> Vec<Declaration> {
    let mut children = Vec::new();
    for c in block.children() {
        if !c.is_named() {
            continue;
        }
        if let Some(decl) = _node_to_decl(&c, src, true) {
            children.push(decl);
        }
    }
    children
}

fn _node_to_decl<'a, D: Doc>(
    node: &Node<'a, D>,
    src: &[u8],
    inside_class: bool,
) -> Option<Declaration> {
    let kind = node.kind();

    if kind == "decorated_definition" {
        let mut decorators = Vec::new();
        for c in node.children() {
            if c.kind() == "decorator" {
                decorators.push(collapse_ws(&c.text()));
            }
        }
        let definition = node.field("definition")?;
        let mut decl = _node_to_decl(&definition, src, inside_class)?;

        let mut new_attrs = decorators.clone();
        new_attrs.extend(decl.attrs);
        decl.attrs = new_attrs;

        decl.start_line = node.start_pos().line() + 1;
        decl.start_byte = node.range().start;
        let ds_byte = if decl.doc_start_byte > 0 {
            std::cmp::min(decl.doc_start_byte, node.range().start)
        } else {
            node.range().start
        };
        decl.doc_start_byte = ds_byte;

        if inside_class
            && decl.kind == DeclarationKind::Method
            && decorators.iter().any(|d| {
                d == "@property" || d.starts_with("@property ") || d.starts_with("@property\n")
            })
        {
            decl.kind = DeclarationKind::Property;
        }
        return Some(decl);
    }

    if kind == "class_definition" {
        return Some(_class_to_decl(node, src));
    }

    if kind == "function_definition" {
        return Some(_function_to_decl(node, src, inside_class));
    }

    if kind == "expression_statement" {
        for inner in node.children() {
            if inner.kind() == "assignment" {
                return _assignment_to_decl(&inner, src);
            }
        }
        return None;
    }

    if kind == "assignment" {
        return _assignment_to_decl(node, src);
    }

    None
}

fn _class_to_decl<'a, D: Doc>(node: &Node<'a, D>, src: &[u8]) -> Declaration {
    let name = field_text(node, "name").unwrap_or_else(|| "?".to_string());
    let bases = _class_bases(node, src);
    let body = node.field("body");

    let docs = body
        .as_ref()
        .map(|b| _docstring(b, src))
        .unwrap_or_default();
    let children = body
        .as_ref()
        .map(|b| _walk_class_body(b, src))
        .unwrap_or_default();

    let mut sig = format!("class {}", name);
    if !bases.is_empty() {
        sig.push('(');
        sig.push_str(&bases.join(", "));
        sig.push(')');
    }

    let range = node.range();
    Declaration {
        kind: DeclarationKind::Class,
        name: name.clone(),
        signature: sig,
        bases,
        attrs: Vec::new(),
        docs,
        docs_inside: true,
        visibility: _visibility_for_name(&name),
        start_line: node.start_pos().line() + 1,
        end_line: node.end_pos().line() + 1,
        start_byte: range.start,
        end_byte: range.end,
        doc_start_byte: range.start,
        native_kind: None,
        modifiers: Vec::new(),
        deprecated: false,
        children,
    }
}

fn _function_to_decl<'a, D: Doc>(
    node: &Node<'a, D>,
    src: &[u8],
    inside_class: bool,
) -> Declaration {
    let name = field_text(node, "name").unwrap_or_else(|| "?".to_string());
    let body = node.field("body");

    let docs = body
        .as_ref()
        .map(|b| _docstring(b, src))
        .unwrap_or_default();
    let sig = _function_signature(node, src);

    let kind = if inside_class && name == "__init__" {
        DeclarationKind::Constructor
    } else if inside_class {
        DeclarationKind::Method
    } else {
        DeclarationKind::Function
    };

    let range = node.range();
    Declaration {
        kind,
        name: name.clone(),
        signature: sig,
        bases: Vec::new(),
        attrs: Vec::new(),
        docs,
        docs_inside: true,
        visibility: _visibility_for_name(&name),
        start_line: node.start_pos().line() + 1,
        end_line: node.end_pos().line() + 1,
        start_byte: range.start,
        end_byte: range.end,
        doc_start_byte: range.start,
        native_kind: None,
        modifiers: Vec::new(),
        deprecated: false,
        children: Vec::new(),
    }
}

fn _assignment_to_decl<'a, D: Doc>(node: &Node<'a, D>, _src: &[u8]) -> Option<Declaration> {
    let left = node.field("left")?;
    if left.kind() != "identifier" {
        return None;
    }
    let name = left.text().into_owned();

    let type_str = node
        .field("type")
        .map(|t| t.text().trim_start_matches(':').trim().to_string());

    let sig = if let Some(t) = type_str {
        format!("{}: {}", name, t)
    } else {
        name.clone()
    };

    let range = node.range();
    Some(Declaration {
        kind: DeclarationKind::Field,
        name: name.clone(),
        signature: sig,
        bases: Vec::new(),
        attrs: Vec::new(),
        docs: Vec::new(),
        docs_inside: false,
        visibility: _visibility_for_name(&name),
        start_line: node.start_pos().line() + 1,
        end_line: node.end_pos().line() + 1,
        start_byte: range.start,
        end_byte: range.end,
        doc_start_byte: 0,
        native_kind: None,
        modifiers: Vec::new(),
        deprecated: false,
        children: Vec::new(),
    })
}

fn _function_signature<'a, D: Doc>(node: &Node<'a, D>, src: &[u8]) -> String {
    let body = node.field("body");
    let end_byte = body.map(|b| b.range().start).unwrap_or(node.range().end);
    let start_byte = node.range().start;

    let text = String::from_utf8_lossy(&src[start_byte..end_byte]).to_string();
    let text = collapse_ws(&text);
    text.trim_end_matches(&[' ', ':'][..]).to_string()
}

fn _class_bases<'a, D: Doc>(node: &Node<'a, D>, _src: &[u8]) -> Vec<String> {
    let sup = node.field("superclasses");
    let mut out = Vec::new();
    if let Some(s) = sup {
        for c in s.children() {
            if !c.is_named() {
                continue;
            }
            if c.kind() == "keyword_argument" {
                continue;
            }
            let t = collapse_ws(&c.text());
            if !t.is_empty() {
                out.push(t);
            }
        }
    }
    out
}

fn _docstring<'a, D: Doc>(block: &Node<'a, D>, _src: &[u8]) -> Vec<String> {
    for c in block.children() {
        if !c.is_named() {
            continue;
        }
        if c.kind() == "expression_statement" {
            let mut inner = c.children().filter(|child| child.is_named());
            if let Some(i) = inner.next() {
                if i.kind() == "string" || i.kind() == "concatenated_string" {
                    let text = i.text().into_owned();
                    return text.lines().map(|s| s.to_string()).collect();
                }
            }
        }
        break;
    }
    Vec::new()
}

fn _visibility_for_name(name: &str) -> String {
    if name.starts_with("__") && name.ends_with("__") {
        String::new()
    } else if name.starts_with('_') {
        "private".to_string()
    } else {
        String::new()
    }
}

use super::base::{collapse_ws, count_parse_errors, field_text, LanguageAdapter};
use crate::core::declaration::{Declaration, DeclarationKind, ParseResult};
use ast_grep_core::{Doc, Node};
use std::path::Path;

pub struct RustAdapter;

impl LanguageAdapter for RustAdapter {
    fn language_name(&self) -> &'static str {
        "rust"
    }

    fn parse<'a, D: Doc>(&self, path: &Path, source: &[u8], root: Node<'a, D>) -> ParseResult {
        let mut decls = Vec::new();
        _walk_mod(&root, source, &mut decls);
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

/// Walk a module (or the file root) in two passes:
/// 1. Emit every top-level decl as today, EXCEPT `impl_item` which is
///    held aside in `pending_impls`.
/// 2. Distribute each pending impl into its target type's `bases` /
///    `children`. Impls whose target isn't declared in this scope (e.g.
///    `impl Display for Foo` where Foo lives in another crate) fall
///    through as a synthesized top-level decl, matching the pre-rewrite
///    behaviour so we never lose info.
///
/// `ast-outline implements Trait` now finds the *struct*, not a
/// synthetic `impl_Foo` shadow.
fn _walk_mod<'a, D: Doc>(node: &Node<'a, D>, src: &[u8], out: &mut Vec<Declaration>) {
    let mut pending_impls: Vec<Declaration> = Vec::new();

    for child in node.children() {
        if !child.is_named() {
            continue;
        }
        if child.kind() == "impl_item" {
            pending_impls.push(_impl_to_decl(&child, src));
        } else if let Some(decl) = _node_to_decl(&child, src) {
            out.push(decl);
        }
    }

    for impl_decl in pending_impls {
        // `_impl_to_decl` synthesises a name like `impl_Foo`; the real
        // target is the suffix.
        let target_name = impl_decl
            .name
            .strip_prefix("impl_")
            .unwrap_or(&impl_decl.name)
            .to_string();

        if let Some(target) = out
            .iter_mut()
            .find(|d| d.name == target_name && _is_regroup_target(&d.kind))
        {
            // Trait impl: lift the trait into the target's `bases` so
            // `find_implementations` traverses Foo, not impl_Foo.
            for b in impl_decl.bases {
                if !target.bases.contains(&b) {
                    target.bases.push(b);
                }
            }
            // Inherent or trait impl: methods become members of the type.
            target.children.extend(impl_decl.children);
        } else {
            // Target type lives elsewhere (cross-crate / foreign type).
            // Keep the synthesized decl so the methods aren't lost.
            out.push(impl_decl);
        }
    }
}

fn _is_regroup_target(kind: &DeclarationKind) -> bool {
    matches!(
        kind,
        DeclarationKind::Struct
            | DeclarationKind::Enum
            | DeclarationKind::Interface
            | DeclarationKind::Class
            | DeclarationKind::Record
    )
}

fn _node_to_decl<'a, D: Doc>(node: &Node<'a, D>, src: &[u8]) -> Option<Declaration> {
    let kind = node.kind();

    if kind == "struct_item" {
        return Some(_struct_to_decl(node, src));
    }
    if kind == "enum_item" {
        return Some(_enum_to_decl(node, src));
    }
    if kind == "trait_item" {
        return Some(_trait_to_decl(node, src));
    }
    if kind == "function_item" {
        return Some(_function_to_decl(node, src, false));
    }
    if kind == "mod_item" {
        return Some(_mod_to_decl(node, src));
    }
    if kind == "macro_definition" {
        return Some(_macro_to_decl(node, src));
    }
    if kind == "foreign_mod_item" {
        return Some(_foreign_mod_to_decl(node, src));
    }
    if kind == "union_item" {
        // Treated as a struct for outline purposes — same shape as far
        // as users navigating an outline care.
        return Some(_struct_to_decl(node, src));
    }

    None
}

fn _struct_to_decl<'a, D: Doc>(node: &Node<'a, D>, src: &[u8]) -> Declaration {
    let name = field_text(node, "name").unwrap_or_else(|| "?".to_string());

    let mut attrs = Vec::new();
    let mut docs = Vec::new();
    _extract_attrs_and_docs(node, src, &mut attrs, &mut docs);

    let mut children = Vec::new();
    if let Some(body) = node.field("body") {
        match body.kind().as_ref() {
            "field_declaration_list" => {
                for field in body.children() {
                    if field.kind() == "field_declaration" {
                        if let Some(fd) = _field_to_decl(&field, src) {
                            children.push(fd);
                        }
                    }
                }
            }
            "ordered_field_declaration_list" => {
                // Tuple struct: tree-sitter renders the body as a flat
                // sequence of `visibility_modifier?` + type nodes (no
                // `field_declaration` wrapper). Track the running visibility
                // and emit one Field per type, with synthetic name "0", "1",…
                // so users can navigate `pair.0` style.
                let mut pending_vis = String::new();
                let mut pending_attrs: Vec<String> = Vec::new();
                let mut idx = 0usize;
                for c in body.children() {
                    if !c.is_named() {
                        continue;
                    }
                    let k = c.kind();
                    if k == "visibility_modifier" {
                        pending_vis = collapse_ws(&c.text());
                        continue;
                    }
                    if k == "attribute_item" {
                        pending_attrs.push(collapse_ws(&c.text()));
                        continue;
                    }
                    children.push(_positional_field_to_decl(
                        &c,
                        src,
                        idx,
                        std::mem::take(&mut pending_vis),
                        std::mem::take(&mut pending_attrs),
                    ));
                    idx += 1;
                }
            }
            _ => {}
        }
    }
    // Unit structs (`struct Foo;`) have no body field — children stays empty,
    // which is the correct outline.

    let sig_end = node
        .field("body")
        .map(|b| b.range().start)
        .unwrap_or(node.range().end);
    let sig = collapse_ws(&String::from_utf8_lossy(&src[node.range().start..sig_end]))
        .trim_end_matches(&[' ', '{', ';'][..])
        .to_string();

    Declaration {
        kind: DeclarationKind::Struct,
        name,
        signature: sig,
        bases: Vec::new(),
        deprecated: false,
        attrs,
        docs,
        docs_inside: false,
        visibility: _visibility(node, src),
        start_line: node.start_pos().line() + 1,
        end_line: node.end_pos().line() + 1,
        start_byte: node.range().start,
        end_byte: node.range().end,
        doc_start_byte: _doc_start(node),
        native_kind: None,
        modifiers: Vec::new(),
        children,
    }
}

fn _enum_to_decl<'a, D: Doc>(node: &Node<'a, D>, src: &[u8]) -> Declaration {
    let name = field_text(node, "name").unwrap_or_else(|| "?".to_string());

    let mut attrs = Vec::new();
    let mut docs = Vec::new();
    _extract_attrs_and_docs(node, src, &mut attrs, &mut docs);

    let mut children = Vec::new();
    if let Some(body) = node.field("body") {
        for variant in body.children() {
            if variant.kind() == "enum_variant" {
                let vname = field_text(&variant, "name").unwrap_or_else(|| "?".to_string());
                let vr = variant.range();
                children.push(Declaration {
                    kind: DeclarationKind::EnumMember,
                    name: vname.clone(),
                    signature: vname,
                    bases: Vec::new(),
                    attrs: Vec::new(),
                    docs: Vec::new(),
                    docs_inside: false,
                    visibility: String::new(),
                    start_line: variant.start_pos().line() + 1,
                    end_line: variant.end_pos().line() + 1,
                    start_byte: vr.start,
                    end_byte: vr.end,
                    doc_start_byte: vr.start,
                    native_kind: None,
                    modifiers: Vec::new(),
                    deprecated: false,
                    children: Vec::new(),
                });
            }
        }
    }

    let sig_end = node
        .field("body")
        .map(|b| b.range().start)
        .unwrap_or(node.range().end);
    let sig = collapse_ws(&String::from_utf8_lossy(&src[node.range().start..sig_end]))
        .trim_end_matches(&[' ', '{'][..])
        .to_string();

    Declaration {
        kind: DeclarationKind::Enum,
        name,
        signature: sig,
        bases: Vec::new(),
        deprecated: false,
        attrs,
        docs,
        docs_inside: false,
        visibility: _visibility(node, src),
        start_line: node.start_pos().line() + 1,
        end_line: node.end_pos().line() + 1,
        start_byte: node.range().start,
        end_byte: node.range().end,
        doc_start_byte: _doc_start(node),
        native_kind: None,
        modifiers: Vec::new(),
        children,
    }
}

fn _trait_to_decl<'a, D: Doc>(node: &Node<'a, D>, src: &[u8]) -> Declaration {
    let name = field_text(node, "name").unwrap_or_else(|| "?".to_string());

    let mut attrs = Vec::new();
    let mut docs = Vec::new();
    _extract_attrs_and_docs(node, src, &mut attrs, &mut docs);

    let mut children = Vec::new();
    if let Some(body) = node.field("body") {
        for item in body.children() {
            match item.kind().as_ref() {
                "function_signature_item" | "function_item" => {
                    children.push(_function_to_decl(&item, src, true));
                }
                "associated_type" => {
                    if let Some(d) = _associated_type_to_decl(&item, src) {
                        children.push(d);
                    }
                }
                "const_item" => {
                    if let Some(d) = _const_or_static_to_field(&item, src) {
                        children.push(d);
                    }
                }
                _ => {}
            }
        }
    }

    let sig_end = node
        .field("body")
        .map(|b| b.range().start)
        .unwrap_or(node.range().end);
    let sig = collapse_ws(&String::from_utf8_lossy(&src[node.range().start..sig_end]))
        .trim_end_matches(&[' ', '{'][..])
        .to_string();

    Declaration {
        kind: DeclarationKind::Interface,
        name,
        signature: sig,
        bases: Vec::new(),
        deprecated: false,
        attrs,
        docs,
        docs_inside: false,
        visibility: _visibility(node, src),
        start_line: node.start_pos().line() + 1,
        end_line: node.end_pos().line() + 1,
        start_byte: node.range().start,
        end_byte: node.range().end,
        doc_start_byte: _doc_start(node),
        native_kind: None,
        modifiers: Vec::new(),
        children,
    }
}

fn _impl_to_decl<'a, D: Doc>(node: &Node<'a, D>, src: &[u8]) -> Declaration {
    let name = field_text(node, "type").unwrap_or_else(|| "?".to_string());
    let trait_node = node.field("trait");
    let trait_name = trait_node.map(|t| collapse_ws(&t.text()));

    let mut attrs = Vec::new();
    let mut docs = Vec::new();
    _extract_attrs_and_docs(node, src, &mut attrs, &mut docs);

    let mut children = Vec::new();
    if let Some(body) = node.field("body") {
        for item in body.children() {
            if item.kind() == "function_item" {
                children.push(_function_to_decl(&item, src, true));
            }
        }
    }

    let mut sig = "impl ".to_string();
    if let Some(t) = &trait_name {
        sig.push_str(t);
        sig.push_str(" for ");
    }
    sig.push_str(&name);

    Declaration {
        kind: DeclarationKind::Class,
        name: format!("impl_{}", name),
        signature: sig,
        bases: trait_name.into_iter().collect(),
        deprecated: false,
        attrs,
        docs,
        docs_inside: false,
        visibility: String::new(),
        start_line: node.start_pos().line() + 1,
        end_line: node.end_pos().line() + 1,
        start_byte: node.range().start,
        end_byte: node.range().end,
        doc_start_byte: _doc_start(node),
        native_kind: None,
        modifiers: Vec::new(),
        children,
    }
}

fn _function_to_decl<'a, D: Doc>(node: &Node<'a, D>, src: &[u8], is_method: bool) -> Declaration {
    let name = field_text(node, "name").unwrap_or_else(|| "?".to_string());

    let mut attrs = Vec::new();
    let mut docs = Vec::new();
    _extract_attrs_and_docs(node, src, &mut attrs, &mut docs);

    let sig_end = node
        .field("body")
        .map(|b| b.range().start)
        .unwrap_or(node.range().end);
    let sig = collapse_ws(&String::from_utf8_lossy(&src[node.range().start..sig_end]))
        .trim_end_matches(&[' ', '{', ';'][..])
        .to_string();

    Declaration {
        kind: if is_method {
            DeclarationKind::Method
        } else {
            DeclarationKind::Function
        },
        name,
        signature: sig,
        bases: Vec::new(),
        deprecated: false,
        attrs,
        docs,
        docs_inside: false,
        visibility: _visibility(node, src),
        start_line: node.start_pos().line() + 1,
        end_line: node.end_pos().line() + 1,
        start_byte: node.range().start,
        end_byte: node.range().end,
        doc_start_byte: _doc_start(node),
        native_kind: None,
        modifiers: Vec::new(),
        children: Vec::new(),
    }
}

fn _mod_to_decl<'a, D: Doc>(node: &Node<'a, D>, src: &[u8]) -> Declaration {
    let name = field_text(node, "name").unwrap_or_else(|| "?".to_string());

    let mut attrs = Vec::new();
    let mut docs = Vec::new();
    _extract_attrs_and_docs(node, src, &mut attrs, &mut docs);

    let mut children = Vec::new();
    if let Some(body) = node.field("body") {
        _walk_mod(&body, src, &mut children);
    }

    let sig_end = node
        .field("body")
        .map(|b| b.range().start)
        .unwrap_or(node.range().end);
    let sig = collapse_ws(&String::from_utf8_lossy(&src[node.range().start..sig_end]))
        .trim_end_matches(&[' ', '{', ';'][..])
        .to_string();

    Declaration {
        kind: DeclarationKind::Namespace,
        name: name.clone(),
        signature: sig,
        bases: Vec::new(),
        deprecated: false,
        attrs,
        docs,
        docs_inside: false,
        visibility: _visibility(node, src),
        start_line: node.start_pos().line() + 1,
        end_line: node.end_pos().line() + 1,
        start_byte: node.range().start,
        end_byte: node.range().end,
        doc_start_byte: _doc_start(node),
        native_kind: None,
        modifiers: Vec::new(),
        children,
    }
}

fn _macro_to_decl<'a, D: Doc>(node: &Node<'a, D>, src: &[u8]) -> Declaration {
    let name = field_text(node, "name").unwrap_or_else(|| "?".to_string());

    let mut attrs = Vec::new();
    let mut docs = Vec::new();
    _extract_attrs_and_docs(node, src, &mut attrs, &mut docs);

    let visibility = if attrs.iter().any(|a| a.contains("macro_export")) {
        "pub".to_string()
    } else {
        String::new()
    };

    let sig = format!("macro_rules! {}", name);

    Declaration {
        kind: DeclarationKind::Delegate,
        name,
        signature: sig,
        bases: Vec::new(),
        deprecated: false,
        attrs,
        docs,
        docs_inside: false,
        visibility,
        start_line: node.start_pos().line() + 1,
        end_line: node.end_pos().line() + 1,
        start_byte: node.range().start,
        end_byte: node.range().end,
        doc_start_byte: _doc_start(node),
        native_kind: None,
        modifiers: Vec::new(),
        children: Vec::new(),
    }
}

/// `extern "C" { fn foo(...); static BAR: T; }` — surface the FFI block
/// as a Namespace named after the ABI string, with each foreign item as
/// a child function/field.
fn _foreign_mod_to_decl<'a, D: Doc>(node: &Node<'a, D>, src: &[u8]) -> Declaration {
    // `extern_modifier` is the `extern "C"` (or `extern "system"`, …) prefix.
    let abi = node
        .children()
        .find(|c| c.kind() == "extern_modifier")
        .map(|n| collapse_ws(&n.text()))
        .unwrap_or_else(|| "extern".to_string());

    let mut attrs = Vec::new();
    let mut docs = Vec::new();
    _extract_attrs_and_docs(node, src, &mut attrs, &mut docs);

    let mut children = Vec::new();
    // The body is a `declaration_list` direct child of `foreign_mod_item`.
    for body in node.children().filter(|c| c.kind() == "declaration_list") {
        for item in body.children() {
            match item.kind().as_ref() {
                "function_signature_item" => {
                    children.push(_function_to_decl(&item, src, false));
                }
                "static_item" => {
                    if let Some(d) = _const_or_static_to_field(&item, src) {
                        children.push(d);
                    }
                }
                _ => {}
            }
        }
    }

    Declaration {
        kind: DeclarationKind::Namespace,
        name: abi.clone(),
        signature: abi,
        bases: Vec::new(),
        deprecated: false,
        attrs,
        docs,
        docs_inside: false,
        visibility: _visibility(node, src),
        start_line: node.start_pos().line() + 1,
        end_line: node.end_pos().line() + 1,
        start_byte: node.range().start,
        end_byte: node.range().end,
        doc_start_byte: _doc_start(node),
        native_kind: None,
        modifiers: Vec::new(),
        children,
    }
}

fn _associated_type_to_decl<'a, D: Doc>(node: &Node<'a, D>, src: &[u8]) -> Option<Declaration> {
    let name = field_text(node, "name")?;
    let mut attrs = Vec::new();
    let mut docs = Vec::new();
    _extract_attrs_and_docs(node, src, &mut attrs, &mut docs);

    let sig = collapse_ws(&String::from_utf8_lossy(
        &src[node.range().start..node.range().end],
    ))
    .trim_end_matches(';')
    .to_string();

    Some(Declaration {
        kind: DeclarationKind::Field,
        name,
        signature: sig,
        bases: Vec::new(),
        deprecated: false,
        attrs,
        docs,
        docs_inside: false,
        visibility: _visibility(node, src),
        start_line: node.start_pos().line() + 1,
        end_line: node.end_pos().line() + 1,
        start_byte: node.range().start,
        end_byte: node.range().end,
        doc_start_byte: _doc_start(node),
        native_kind: None,
        modifiers: Vec::new(),
        children: Vec::new(),
    })
}

fn _const_or_static_to_field<'a, D: Doc>(node: &Node<'a, D>, src: &[u8]) -> Option<Declaration> {
    let name = field_text(node, "name")?;
    let mut attrs = Vec::new();
    let mut docs = Vec::new();
    _extract_attrs_and_docs(node, src, &mut attrs, &mut docs);

    let sig = collapse_ws(&String::from_utf8_lossy(
        &src[node.range().start..node.range().end],
    ))
    .trim_end_matches(';')
    .to_string();

    Some(Declaration {
        kind: DeclarationKind::Field,
        name,
        signature: sig,
        bases: Vec::new(),
        deprecated: false,
        attrs,
        docs,
        docs_inside: false,
        visibility: _visibility(node, src),
        start_line: node.start_pos().line() + 1,
        end_line: node.end_pos().line() + 1,
        start_byte: node.range().start,
        end_byte: node.range().end,
        doc_start_byte: _doc_start(node),
        native_kind: None,
        modifiers: Vec::new(),
        children: Vec::new(),
    })
}

fn _field_to_decl<'a, D: Doc>(node: &Node<'a, D>, src: &[u8]) -> Option<Declaration> {
    let name = field_text(node, "name")?;
    let mut attrs = Vec::new();
    let mut docs = Vec::new();
    _extract_attrs_and_docs(node, src, &mut attrs, &mut docs);

    let sig = collapse_ws(&String::from_utf8_lossy(
        &src[node.range().start..node.range().end],
    ))
    .trim_end_matches(',')
    .to_string();

    Some(Declaration {
        kind: DeclarationKind::Field,
        name,
        signature: sig,
        bases: Vec::new(),
        deprecated: false,
        attrs,
        docs,
        docs_inside: false,
        visibility: _visibility(node, src),
        start_line: node.start_pos().line() + 1,
        end_line: node.end_pos().line() + 1,
        start_byte: node.range().start,
        end_byte: node.range().end,
        doc_start_byte: _doc_start(node),
        native_kind: None,
        modifiers: Vec::new(),
        children: Vec::new(),
    })
}

/// Tuple-struct positional field. Tree-sitter doesn't wrap these in
/// `field_declaration` nodes — `pub struct Pair(pub u8, i32)` parses as
/// alternating `visibility_modifier` + type nodes. Caller hands us the
/// type node, the running visibility, and any preceding attrs.
fn _positional_field_to_decl<'a, D: Doc>(
    node: &Node<'a, D>,
    src: &[u8],
    idx: usize,
    visibility: String,
    attrs: Vec<String>,
) -> Declaration {
    let type_text = collapse_ws(&String::from_utf8_lossy(
        &src[node.range().start..node.range().end],
    ));
    // Prefix the index so the outline renderer (which renders fields by
    // signature, not name) shows `0: pub u8` instead of just `pub u8`.
    let sig = if !visibility.is_empty() {
        format!("{}: {} {}", idx, visibility, type_text)
    } else {
        format!("{}: {}", idx, type_text)
    };

    Declaration {
        kind: DeclarationKind::Field,
        name: idx.to_string(),
        signature: sig,
        bases: Vec::new(),
        deprecated: false,
        attrs,
        docs: Vec::new(),
        docs_inside: false,
        visibility,
        start_line: node.start_pos().line() + 1,
        end_line: node.end_pos().line() + 1,
        start_byte: node.range().start,
        end_byte: node.range().end,
        doc_start_byte: node.range().start,
        native_kind: None,
        modifiers: Vec::new(),
        children: Vec::new(),
    }
}

fn _extract_attrs_and_docs<'a, D: Doc>(
    node: &Node<'a, D>,
    _src: &[u8],
    attrs: &mut Vec<String>,
    docs: &mut Vec<String>,
) {
    let mut current = node.prev();
    let mut nodes = Vec::new();
    while let Some(prev) = current {
        if prev.kind() == "line_comment"
            || prev.kind() == "block_comment"
            || prev.kind() == "attribute_item"
        {
            nodes.push(prev.clone());
            current = prev.prev();
        } else {
            break;
        }
    }
    nodes.reverse();
    for n in nodes {
        if n.kind() == "attribute_item" {
            attrs.push(collapse_ws(&n.text()));
        } else {
            let t = n.text().into_owned();
            if t.starts_with("///") || t.starts_with("/**") {
                docs.push(t);
            }
        }
    }
}

fn _doc_start<'a, D: Doc>(node: &Node<'a, D>) -> usize {
    let mut start = node.range().start;
    let mut current = node.prev();
    while let Some(prev) = current {
        if prev.kind() == "line_comment"
            || prev.kind() == "block_comment"
            || prev.kind() == "attribute_item"
        {
            start = prev.range().start;
            current = prev.prev();
        } else {
            break;
        }
    }
    start
}

fn _visibility<'a, D: Doc>(node: &Node<'a, D>, _src: &[u8]) -> String {
    for c in node.children() {
        if c.kind() == "visibility_modifier" {
            return collapse_ws(&c.text());
        }
    }
    String::new() // Rust default is private
}

use repolayer::core::declaration::{Declaration, DeclarationKind, OutlineOptions, ParseResult};
use repolayer::outline::render::render_outline;
use std::path::PathBuf;

#[test]
fn renders_minimal_outline() {
    let pr = ParseResult {
        path: PathBuf::from("foo.rs"),
        language: "rust",
        source: b"".to_vec(),
        line_count: 5,
        error_count: 0,
        declarations: vec![Declaration {
            kind: DeclarationKind::Function,
            name: "foo".into(),
            signature: "pub fn foo()".into(),
            start_line: 1,
            end_line: 1,
            ..Default::default()
        }],
    };
    let out = render_outline(&pr, &OutlineOptions::default());
    assert!(out.contains("foo.rs"), "output: {}", out);
    assert!(out.contains("pub fn foo()"), "output: {}", out);
}

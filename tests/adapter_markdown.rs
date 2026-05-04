use repolayer::adapters::markdown::parse_markdown;
use repolayer::core::declaration::DeclarationKind;
use std::path::Path;

#[test]
fn parses_headings() {
    let src = "# Title\n\n## Section A\n\n### Sub A1\n\n## Section B\n";
    let r = parse_markdown(Path::new("doc.md"), src.as_bytes());
    let names: Vec<_> = r.declarations.iter().map(|d| d.name.clone()).collect();
    assert!(names.contains(&"Title".to_string()), "got: {:?}", names);
    let title = r.declarations.iter().find(|d| d.name == "Title").unwrap();
    assert!(matches!(title.kind, DeclarationKind::Heading));
}

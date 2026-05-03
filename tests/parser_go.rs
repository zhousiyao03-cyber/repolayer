use repolayer::parser::go::GoParser;
use repolayer::parser::Parser;
use std::path::PathBuf;

#[test]
fn extracts_exported_funcs_and_types() {
    let p = GoParser::new();
    let r = p
        .parse_file(&PathBuf::from("tests/fixtures/single_repo_go/main.go"))
        .unwrap();
    let names: Vec<_> = r.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"Login"), "uppercase = exported");
    assert!(names.contains(&"Auth"));
    assert!(names.contains(&"SmallType"));
    assert!(!names.contains(&"helper"), "lowercase = unexported");
    assert!(!names.contains(&"unexported"));
    assert!(
        names.contains(&"ID"),
        "exported type alias must be indexed (was the regression)"
    );
    assert!(
        !names.contains(&"smallAlias"),
        "unexported type alias must be skipped"
    );
}

#[test]
fn extracts_imports_single_and_block() {
    let p = GoParser::new();
    let r = p
        .parse_file(&PathBuf::from("tests/fixtures/single_repo_go/main.go"))
        .unwrap();
    assert!(r.imports.contains(&"fmt".to_string()));
    assert!(r.imports.contains(&"errors".to_string()));
    assert!(r.imports.contains(&"strings".to_string()));
}

#[test]
fn extracts_exported_methods() {
    let p = GoParser::new();
    let r = p
        .parse_file(&PathBuf::from("tests/fixtures/single_repo_go/main.go"))
        .unwrap();
    let names: Vec<_> = r.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(
        names.contains(&"Validate"),
        "exported method must be indexed"
    );
    assert!(
        !names.contains(&"internal"),
        "lowercase method must be skipped"
    );
}

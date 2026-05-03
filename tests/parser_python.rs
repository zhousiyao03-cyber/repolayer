use repolayer::parser::python::PythonParser;
use repolayer::parser::Parser;
use std::path::PathBuf;

#[test]
fn extracts_top_level_funcs_and_classes() {
    let p = PythonParser::new();
    let result = p
        .parse_file(&PathBuf::from("tests/fixtures/single_repo_py/main.py"))
        .unwrap();
    let names: Vec<_> = result.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"login"));
    assert!(names.contains(&"Auth"));
    assert!(
        !names.contains(&"_internal"),
        "underscore-prefixed must be skipped"
    );
}

#[test]
fn extracts_from_imports() {
    let p = PythonParser::new();
    let result = p
        .parse_file(&PathBuf::from("tests/fixtures/single_repo_py/main.py"))
        .unwrap();
    assert_eq!(result.imports, vec!["utils".to_string()]);
}

#[test]
fn extracts_decorated_top_level_definitions() {
    let p = PythonParser::new();
    let result = p
        .parse_file(&PathBuf::from("tests/fixtures/single_repo_py/main.py"))
        .unwrap();
    let names: Vec<_> = result.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(
        names.contains(&"public_decorated"),
        "decorated functions must be indexed (was the regression)"
    );
    assert!(
        names.contains(&"stacked_decorators"),
        "stacked-decorator functions must be indexed"
    );
    assert!(
        names.contains(&"DecoratedCls"),
        "decorated classes must be indexed"
    );
    assert!(
        !names.contains(&"_private_decorated"),
        "underscore filter must apply to decorated definitions too"
    );
    // helper functions defined at module top-level shouldn't accidentally double-count
    let public_decorated_count = result
        .symbols
        .iter()
        .filter(|s| s.name == "public_decorated")
        .count();
    assert_eq!(public_decorated_count, 1, "no double-count");
}

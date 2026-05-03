use repolayer::parser::typescript::TypeScriptParser;
use repolayer::parser::Parser;
use std::path::PathBuf;

#[test]
fn parses_exported_functions_only() {
    let p = TypeScriptParser::new();
    let file = PathBuf::from("tests/fixtures/single_repo_ts/src/auth.ts");
    let result = p.parse_file(&file).unwrap();
    let names: Vec<_> = result.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"login"));
    assert!(names.contains(&"logout"));
    assert!(
        !names.contains(&"internalHelper"),
        "internal funcs must not be indexed"
    );
}

#[test]
fn extracts_imports() {
    let p = TypeScriptParser::new();
    let file = PathBuf::from("tests/fixtures/single_repo_ts/src/auth.ts");
    let result = p.parse_file(&file).unwrap();
    assert_eq!(result.imports, vec!["./utils".to_string()]);
}

#[test]
fn extracts_all_declarators_in_multi_const() {
    let p = TypeScriptParser::new();
    let result = p
        .parse_file(&PathBuf::from("tests/fixtures/single_repo_ts/src/multi.ts"))
        .unwrap();
    let names: Vec<_> = result.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"a"), "first declarator must be indexed");
    assert!(
        names.contains(&"b"),
        "second declarator must be indexed (was the regression)"
    );
    assert!(names.contains(&"single"));
    assert!(names.contains(&"regular"));
}

#[test]
fn skips_destructuring_patterns_without_polluting_names() {
    let p = TypeScriptParser::new();
    let result = p
        .parse_file(&PathBuf::from("tests/fixtures/single_repo_ts/src/multi.ts"))
        .unwrap();
    for sym in &result.symbols {
        assert!(
            !sym.name.contains('{') && !sym.name.contains('[') && !sym.name.contains(','),
            "destructuring pattern leaked into symbol name: {:?}",
            sym.name
        );
    }
}

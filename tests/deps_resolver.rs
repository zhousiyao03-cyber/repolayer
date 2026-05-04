use repolayer::deps::resolver::{build_suffix_index, resolve, ResolveCtx};
use repolayer::deps::resolver::build::Lang;
use std::fs;
use tempfile::tempdir;

#[test]
fn resolves_typescript_relative_import() {
    let d = tempdir().unwrap();
    fs::create_dir_all(d.path().join("src")).unwrap();
    fs::write(d.path().join("src/foo.ts"), "export const x = 1;\n").unwrap();
    fs::write(d.path().join("src/bar.ts"), "import { x } from './foo';\n").unwrap();

    let idx = build_suffix_index(d.path());
    let ctx = ResolveCtx {
        from_file: &d.path().join("src/bar.ts"),
        lang: Lang::TypeScript,
        alias_prefix: None,
        path_aliases: &[],
    };
    let resolved = resolve("./foo", &ctx, &idx);
    assert!(resolved.is_some(), "expected resolution to find foo.ts");
    let resolved_path = resolved.unwrap();
    assert!(
        resolved_path.ends_with("src/foo.ts"),
        "resolved path should end with src/foo.ts, got: {}",
        resolved_path.display()
    );
}

#[test]
fn resolves_rust_crate_import() {
    let d = tempdir().unwrap();
    fs::create_dir_all(d.path().join("src")).unwrap();
    fs::write(d.path().join("src/lib.rs"), "pub mod utils;\n").unwrap();
    fs::write(d.path().join("src/utils.rs"), "pub fn helper() {}\n").unwrap();
    fs::write(d.path().join("src/main.rs"), "use crate::utils::helper;\n").unwrap();

    let idx = build_suffix_index(d.path());
    let ctx = ResolveCtx {
        from_file: &d.path().join("src/main.rs"),
        lang: Lang::Rust,
        alias_prefix: None,
        path_aliases: &[],
    };
    let resolved = resolve("crate::utils", &ctx, &idx);
    assert!(resolved.is_some(), "expected resolution to find utils.rs");
}

#[test]
fn returns_none_for_unresolvable_import() {
    let d = tempdir().unwrap();
    fs::create_dir_all(d.path().join("src")).unwrap();
    fs::write(d.path().join("src/bar.ts"), "import { x } from 'nonexistent-pkg';\n").unwrap();

    let idx = build_suffix_index(d.path());
    let from = d.path().join("src/bar.ts");
    let ctx = ResolveCtx::new(&from, Lang::TypeScript);
    let resolved = resolve("nonexistent-pkg", &ctx, &idx);
    // bare module specifiers without path aliases should not resolve
    assert!(
        resolved.is_none(),
        "expected None for unresolvable import, got: {:?}",
        resolved
    );
}

#[test]
fn suffix_index_contains_walked_files() {
    let d = tempdir().unwrap();
    fs::create_dir_all(d.path().join("src/utils")).unwrap();
    fs::write(d.path().join("src/utils/helpers.py"), "def greet(): pass\n").unwrap();

    let idx = build_suffix_index(d.path());
    // The suffix index should have entries for `helpers`, `utils/helpers`, `src/utils/helpers`
    assert!(
        idx.lookup("helpers").is_some(),
        "expected suffix index to contain 'helpers'"
    );
    assert!(
        idx.lookup("utils/helpers").is_some(),
        "expected suffix index to contain 'utils/helpers'"
    );
}

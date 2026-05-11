use repolayer::deps::extract::extract;
use repolayer::deps::resolver::build::Lang;
use std::io::Write;
use tempfile::NamedTempFile;

fn write_file(suffix: &str, content: &str) -> NamedTempFile {
    let mut f = tempfile::Builder::new().suffix(suffix).tempfile().unwrap();
    f.write_all(content.as_bytes()).unwrap();
    f
}

#[test]
fn extracts_typescript_imports() {
    let f = write_file(
        ".ts",
        "import { Foo } from './foo';\nimport bar from 'lib';\n",
    );
    let imports = extract(f.path(), Lang::TypeScript);
    assert!(
        imports.len() >= 2,
        "got {} imports: {:?}",
        imports.len(),
        imports.iter().map(|i| i.spec.clone()).collect::<Vec<_>>()
    );
    let specs: Vec<_> = imports.iter().map(|i| i.spec.clone()).collect();
    assert!(
        specs.iter().any(|s| s.contains("foo")),
        "specs: {:?}",
        specs
    );
    assert!(
        specs.iter().any(|s| s.contains("lib")),
        "specs: {:?}",
        specs
    );
}

#[test]
fn extracts_python_imports() {
    let f = write_file(".py", "from .core import X\nimport os\n");
    let imports = extract(f.path(), Lang::Python);
    assert!(imports.len() >= 2, "got {} imports", imports.len());
    let specs: Vec<_> = imports.iter().map(|i| i.spec.clone()).collect();
    assert!(
        specs
            .iter()
            .any(|s| s.contains("core") || s.contains(".core")),
        "specs: {:?}",
        specs
    );
    assert!(specs.iter().any(|s| s.contains("os")), "specs: {:?}", specs);
}

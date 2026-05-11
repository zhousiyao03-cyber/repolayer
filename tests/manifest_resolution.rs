use repolayer::deps::manifest::detect_aliases;
use std::fs;
use tempfile::tempdir;

#[test]
fn detects_rust_standalone_package() {
    let d = tempdir().unwrap();
    fs::write(
        d.path().join("Cargo.toml"),
        r#"[package]
name = "myapp"
version = "0.1.0"
edition = "2021"
"#,
    )
    .unwrap();
    let aliases = detect_aliases(d.path());
    assert_eq!(aliases.rust_packages.len(), 1);
    assert_eq!(aliases.rust_packages[0].name, "myapp");
}

#[test]
fn detects_rust_workspace_members() {
    let d = tempdir().unwrap();
    fs::write(
        d.path().join("Cargo.toml"),
        r#"[workspace]
members = ["crates/core", "crates/cli"]
"#,
    )
    .unwrap();
    fs::create_dir_all(d.path().join("crates/core")).unwrap();
    fs::write(
        d.path().join("crates/core/Cargo.toml"),
        r#"[package]
name = "myapp-core"
version = "0.1.0"
edition = "2021"
"#,
    )
    .unwrap();
    fs::create_dir_all(d.path().join("crates/cli")).unwrap();
    fs::write(
        d.path().join("crates/cli/Cargo.toml"),
        r#"[package]
name = "myapp-cli"
version = "0.1.0"
edition = "2021"
"#,
    )
    .unwrap();
    let aliases = detect_aliases(d.path());
    let names: Vec<_> = aliases
        .rust_packages
        .iter()
        .map(|p| p.name.clone())
        .collect();
    assert!(
        names.contains(&"myapp-core".to_string()),
        "got: {:?}",
        names
    );
    assert!(names.contains(&"myapp-cli".to_string()), "got: {:?}", names);
}

#[test]
fn detects_python_pyproject() {
    let d = tempdir().unwrap();
    fs::write(
        d.path().join("pyproject.toml"),
        r#"[project]
name = "myapp"
version = "0.1.0"
"#,
    )
    .unwrap();
    let aliases = detect_aliases(d.path());
    assert_eq!(aliases.python_packages.len(), 1);
    assert_eq!(aliases.python_packages[0].name, "myapp");
}

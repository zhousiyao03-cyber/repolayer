use crate::config::Config;
use crate::deps::manifest::detect_aliases;
use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::warn;

#[derive(Debug, Clone)]
pub struct PackageIndex {
    by_name: HashMap<String, PackageInfo>,
}

#[derive(Debug, Clone)]
pub struct PackageInfo {
    pub repo: String,
    pub root: PathBuf,
    pub main_relative: Option<String>,
}

impl PackageIndex {
    /// Scan all non-IDL repos for package.json and build name → repo map.
    pub fn build(workspace_root: &Path, config: &Config) -> Result<Self> {
        let mut by_name = HashMap::new();
        for r in &config.repos {
            if r.is_idl() {
                continue;
            }
            let root = if r.path.is_absolute() {
                r.path.clone()
            } else {
                workspace_root.join(&r.path)
            };
            let repo_name = r.name.clone().unwrap_or_else(|| {
                root.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "repo".to_string())
            });
            let pkg_json = root.join("package.json");
            if pkg_json.exists() {
                match std::fs::read_to_string(&pkg_json) {
                    Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
                        Ok(json) => {
                            let name = json["name"].as_str().unwrap_or("").to_string();
                            let main = json["main"].as_str().map(String::from);
                            if name.is_empty() {
                                warn!(
                                    "package.json at {} has no name field — skipping",
                                    pkg_json.display()
                                );
                            } else {
                                by_name.insert(
                                    name,
                                    PackageInfo {
                                        repo: repo_name.clone(),
                                        root: root.clone(),
                                        main_relative: main,
                                    },
                                );
                            }
                        }
                        Err(e) => warn!(
                            "skip package.json at {}: invalid JSON ({})",
                            pkg_json.display(),
                            e
                        ),
                    },
                    Err(e) => warn!("skip package.json at {}: {}", pkg_json.display(), e),
                }
            }
            // Extend with Rust (Cargo.toml) and Python (pyproject.toml) packages.
            let aliases = detect_aliases(&root);
            for rust_pkg in &aliases.rust_packages {
                // Only insert if not already present (package.json takes priority).
                by_name.entry(rust_pkg.name.clone()).or_insert_with(|| PackageInfo {
                    repo: repo_name.clone(),
                    root: rust_pkg.root.clone(),
                    main_relative: None,
                });
            }
            for py_pkg in &aliases.python_packages {
                by_name.entry(py_pkg.name.clone()).or_insert_with(|| PackageInfo {
                    repo: repo_name.clone(),
                    root: py_pkg.root.clone(),
                    main_relative: None,
                });
            }
        }
        Ok(Self { by_name })
    }

    /// Resolve an import specifier (e.g. "@org/repo-a" or "@org/repo-a/sub/foo")
    /// to a known package. Subpath imports match the longest prefix.
    pub fn lookup(&self, import_spec: &str) -> Option<&PackageInfo> {
        if let Some(p) = self.by_name.get(import_spec) {
            return Some(p);
        }
        // Try prefix match: @org/lib/sub/foo → @org/lib
        for (name, info) in &self.by_name {
            if import_spec.starts_with(&format!("{}/", name)) {
                return Some(info);
            }
        }
        None
    }
}

use crate::config::Config;
use crate::graph::model::*;
use crate::graph::store::Store;
use crate::parser::{typescript::TypeScriptParser, Parser as _};
use anyhow::{Context, Result};
use ignore::WalkBuilder;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

/// Normalize a relative path to use `/` separators for stable cross-platform IDs.
fn rel_path_str(rel: &Path) -> String {
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

pub struct Indexer {
    pub workspace_root: PathBuf,
    pub store: Store,
    pub config: Config,
}

#[derive(Debug, Default)]
pub struct BuildStats {
    pub nodes: u64,
    pub edges: u64,
}

impl Indexer {
    pub fn new(workspace_root: PathBuf, db_path: PathBuf, config: Config) -> Result<Self> {
        let store = Store::open(&db_path)
            .with_context(|| format!("opening store at {}", db_path.display()))?;
        Ok(Self {
            workspace_root,
            store,
            config,
        })
    }

    pub fn build_all(&mut self) -> Result<BuildStats> {
        let mut stats = BuildStats::default();
        let repos = self.config.repos.clone();
        for repo_cfg in &repos {
            if repo_cfg.is_idl() {
                // IDL repos handled in Task 14
                continue;
            }
            let repo_path = self.resolve_repo_path(&repo_cfg.path);
            let repo_name = repo_cfg.name.clone().unwrap_or_else(|| {
                repo_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "repo".to_string())
            });
            self.index_repo(&repo_name, &repo_path, &mut stats)?;
        }
        Ok(stats)
    }

    fn resolve_repo_path(&self, p: &Path) -> PathBuf {
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            self.workspace_root.join(p)
        }
    }

    fn index_repo(&mut self, repo: &str, root: &Path, stats: &mut BuildStats) -> Result<()> {
        info!("indexing repo {} at {}", repo, root.display());
        let repo_node = Node::new(NodeKind::Repo, repo, "", None);
        self.store.upsert_node(&repo_node)?;
        stats.nodes += 1;

        let ts_parser = TypeScriptParser::new();
        let py_parser = crate::parser::python::PythonParser::new();

        for entry in WalkBuilder::new(root).build() {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    warn!("walk error: {}", e);
                    continue;
                }
            };
            if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                continue;
            }
            let path = entry.path();
            let rel = rel_path_str(path.strip_prefix(root).unwrap_or(path));
            let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");

            let parsed = match ext {
                "ts" | "tsx" | "js" | "jsx" | "mjs" => match ts_parser.parse_file(path) {
                    Ok(p) => p,
                    Err(e) => {
                        warn!("skip {}: {}", path.display(), e);
                        continue;
                    }
                },
                "py" => match py_parser.parse_file(path) {
                    Ok(p) => p,
                    Err(e) => {
                        warn!("skip {}: {}", path.display(), e);
                        continue;
                    }
                },
                _ => continue,
            };

            let module_node = Node::new(NodeKind::Module, repo, &rel, None);
            self.store.upsert_node(&module_node)?;
            self.store.upsert_edge(&Edge {
                from: repo_node.id.clone(),
                to: module_node.id.clone(),
                kind: EdgeKind::Contains,
            })?;
            stats.nodes += 1;
            stats.edges += 1;

            for sym in &parsed.symbols {
                let mut sn = Node::new(NodeKind::Symbol, repo, &rel, Some(&sym.name));
                sn.loc_start = Some(sym.loc_start);
                sn.loc_end = Some(sym.loc_end);
                self.store.upsert_node(&sn)?;
                self.store.upsert_edge(&Edge {
                    from: module_node.id.clone(),
                    to: sn.id.clone(),
                    kind: EdgeKind::Contains,
                })?;
                stats.nodes += 1;
                stats.edges += 1;
            }

            for imp in &parsed.imports {
                if let Some(target_path) = resolve_import(root, path, imp) {
                    let target_rel_path = target_path.strip_prefix(root).unwrap_or(&target_path);
                    let target_rel = rel_path_str(target_rel_path);
                    let target_module = Node::new(NodeKind::Module, repo, &target_rel, None);
                    // upsert_node is idempotent — safe to call even if walker will visit this file later
                    self.store.upsert_node(&target_module)?;
                    // Pre-register Contains edge so no module is ever an orphan. We do NOT
                    // count this in stats.edges: the walker will visit the target file and
                    // count the (idempotent) Contains upsert there, so we avoid double-counting.
                    self.store.upsert_edge(&Edge {
                        from: repo_node.id.clone(),
                        to: target_module.id.clone(),
                        kind: EdgeKind::Contains,
                    })?;
                    self.store.upsert_edge(&Edge {
                        from: module_node.id.clone(),
                        to: target_module.id,
                        kind: EdgeKind::Imports,
                    })?;
                    stats.edges += 1;
                }
                // External package imports (no leading '.') are handled by Task 11 linker.
            }
        }
        Ok(())
    }
}

fn resolve_import(repo_root: &Path, from_file: &Path, spec: &str) -> Option<PathBuf> {
    if !spec.starts_with('.') {
        return None;
    }
    let dir = from_file.parent()?;
    let candidate = dir.join(spec);
    for ext in ["ts", "tsx", "js", "jsx", "mjs"] {
        // Use manual append instead of with_extension() to avoid clobbering dots
        // already in the import path (e.g. "./a.test" -> "./a.test.ts" not "./a.ts").
        let mut with_ext_os = candidate.clone().into_os_string();
        with_ext_os.push(".");
        with_ext_os.push(ext);
        let with_ext = PathBuf::from(with_ext_os);
        if with_ext.exists() && with_ext.starts_with(repo_root) {
            return Some(with_ext);
        }
    }
    let index = candidate.join("index.ts");
    if index.exists() && index.starts_with(repo_root) {
        return Some(index);
    }
    None
}

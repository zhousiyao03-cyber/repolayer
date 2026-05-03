pub mod incremental;

use crate::config::Config;
use crate::graph::model::*;
use crate::graph::store::Store;
use crate::parser::typescript::TypeScriptParser;
use anyhow::{Context, Result};

fn parse_by_extension(
    path: &std::path::Path,
    ts_parser: &crate::parser::typescript::TypeScriptParser,
    py_parser: &crate::parser::python::PythonParser,
    go_parser: &crate::parser::go::GoParser,
) -> Option<Result<crate::parser::ParsedFile>> {
    use crate::parser::Parser as _;
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    match ext {
        "ts" | "tsx" | "js" | "jsx" | "mjs" => Some(ts_parser.parse_file(path)),
        "py" => Some(py_parser.parse_file(path)),
        "go" => Some(go_parser.parse_file(path)),
        _ => None,
    }
}
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
        let pkg_index =
            crate::linker::imports::PackageIndex::build(&self.workspace_root, &self.config)?;
        let repos = self.config.repos.clone();
        for repo_cfg in &repos {
            let repo_path = self.resolve_repo_path(&repo_cfg.path);
            let repo_name = repo_cfg.name.clone().unwrap_or_else(|| {
                repo_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "repo".to_string())
            });
            if repo_cfg.is_idl() {
                self.index_idl_repo(&repo_name, &repo_path, &mut stats)?;
                continue;
            }
            self.index_repo(&repo_name, &repo_path, &pkg_index, &mut stats)?;
        }

        // After all repos indexed, link IDL methods to code modules
        let code_repos: Vec<(String, PathBuf)> = self
            .config
            .repos
            .iter()
            .filter(|r| !r.is_idl())
            .map(|r| {
                let root = self.resolve_repo_path(&r.path);
                let name = r.name.clone().unwrap_or_else(|| {
                    root.file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "repo".to_string())
                });
                (name, root)
            })
            .collect();
        let linker = crate::linker::idl_links::IdlLinker {
            store: &self.store,
            repos: code_repos,
        };
        let idl_edges = linker.link_all()?;
        stats.edges += idl_edges;

        // Apply user-declared manual links (repolayer.yml links: section)
        let manual_edges = crate::linker::manual::apply_manual_links(&self.store, &self.config)?;
        stats.edges += manual_edges;

        Ok(stats)
    }

    fn index_idl_repo(&mut self, repo: &str, root: &Path, stats: &mut BuildStats) -> Result<()> {
        use crate::parser::idl::{protobuf::ProtobufParser, thrift::ThriftParser};
        let proto_p = ProtobufParser::new();
        let thrift_p = ThriftParser::new();

        info!("indexing IDL repo {} at {}", repo, root.display());
        let repo_node = Node::new(NodeKind::Repo, repo, "", None);
        self.store.upsert_node(&repo_node)?;
        stats.nodes += 1;

        for entry in WalkBuilder::new(root).build() {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                continue;
            }
            let path = entry.path();
            let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
            let rel = rel_path_str(path.strip_prefix(root).unwrap_or(path));

            let idl = match ext {
                "proto" => match proto_p.parse(path) {
                    Ok(i) => i,
                    Err(e) => {
                        warn!("skip {}: {}", path.display(), e);
                        continue;
                    }
                },
                "thrift" => match thrift_p.parse(path) {
                    Ok(i) => i,
                    Err(e) => {
                        warn!("skip {}: {}", path.display(), e);
                        continue;
                    }
                },
                _ => continue,
            };

            for svc in &idl.services {
                let svc_node = Node::new(NodeKind::IdlService, repo, &rel, Some(&svc.name));
                self.store.upsert_node(&svc_node)?;
                self.store.upsert_edge(&Edge {
                    from: repo_node.id.clone(),
                    to: svc_node.id.clone(),
                    kind: EdgeKind::Defines,
                })?;
                stats.nodes += 1;
                stats.edges += 1;

                for m in &svc.methods {
                    let qualified = format!("{}.{}", svc.name, m.name);
                    let m_node = Node::new(NodeKind::IdlMethod, repo, &rel, Some(&qualified));
                    self.store.upsert_node(&m_node)?;
                    self.store.upsert_edge(&Edge {
                        from: svc_node.id.clone(),
                        to: m_node.id.clone(),
                        kind: EdgeKind::Contains,
                    })?;
                    stats.nodes += 1;
                    stats.edges += 1;
                }
            }
        }
        Ok(())
    }

    pub fn reindex_file(&mut self, repo: &str, abs_path: &Path) -> Result<()> {
        use crate::parser::Parser as _;

        // Find the repo's root from config
        let repo_cfg = self.config.repos.iter().find(|r| {
            let root = self.resolve_repo_path(&r.path);
            root.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default()
                == repo
        });
        let Some(rcfg) = repo_cfg.cloned() else {
            return Ok(());
        };
        let root = self.resolve_repo_path(&rcfg.path);
        let Ok(rel_path) = abs_path.strip_prefix(&root) else {
            return Ok(());
        };
        let rel = rel_path_str(rel_path);

        // Delete existing nodes for this module
        self.store.delete_module(repo, &rel)?;

        // If file no longer exists (deleted), we're done after delete
        if !abs_path.exists() {
            return Ok(());
        }

        let ext = abs_path.extension().and_then(|s| s.to_str()).unwrap_or("");
        let ts_parser = crate::parser::typescript::TypeScriptParser::new();
        let py_parser = crate::parser::python::PythonParser::new();
        let go_parser = crate::parser::go::GoParser::new();

        let parsed = match ext {
            "ts" | "tsx" | "js" | "jsx" | "mjs" => match ts_parser.parse_file(abs_path) {
                Ok(p) => p,
                Err(_) => return Ok(()),
            },
            "py" => match py_parser.parse_file(abs_path) {
                Ok(p) => p,
                Err(_) => return Ok(()),
            },
            "go" => match go_parser.parse_file(abs_path) {
                Ok(p) => p,
                Err(_) => return Ok(()),
            },
            _ => return Ok(()),
        };

        let repo_node = Node::new(NodeKind::Repo, repo, "", None);
        self.store.upsert_node(&repo_node)?;
        let module_node = Node::new(NodeKind::Module, repo, &rel, None);
        self.store.upsert_node(&module_node)?;
        self.store.upsert_edge(&Edge {
            from: repo_node.id,
            to: module_node.id.clone(),
            kind: EdgeKind::Contains,
        })?;
        for sym in &parsed.symbols {
            let mut sn = Node::new(NodeKind::Symbol, repo, &rel, Some(&sym.name));
            sn.loc_start = Some(sym.loc_start);
            sn.loc_end = Some(sym.loc_end);
            self.store.upsert_node(&sn)?;
            self.store.upsert_edge(&Edge {
                from: module_node.id.clone(),
                to: sn.id,
                kind: EdgeKind::Contains,
            })?;
        }
        Ok(())
    }

    fn resolve_repo_path(&self, p: &Path) -> PathBuf {
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            self.workspace_root.join(p)
        }
    }

    fn index_repo(
        &mut self,
        repo: &str,
        root: &Path,
        pkg_index: &crate::linker::imports::PackageIndex,
        stats: &mut BuildStats,
    ) -> Result<()> {
        info!("indexing repo {} at {}", repo, root.display());
        let repo_node = Node::new(NodeKind::Repo, repo, "", None);
        self.store.upsert_node(&repo_node)?;
        stats.nodes += 1;

        let ts_parser = TypeScriptParser::new();
        let py_parser = crate::parser::python::PythonParser::new();
        let go_parser = crate::parser::go::GoParser::new();

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
            let parsed = match parse_by_extension(path, &ts_parser, &py_parser, &go_parser) {
                None => continue,
                Some(Err(e)) => {
                    warn!("skip {}: {}", path.display(), e);
                    continue;
                }
                Some(Ok(p)) => p,
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
                } else if let Some(pkg) = pkg_index.lookup(imp) {
                    // Cross-repo import: link to the target package's main module (or root)
                    let target_rel = pkg
                        .main_relative
                        .clone()
                        .unwrap_or_else(|| "package.json".to_string());
                    let target_module = Node::new(NodeKind::Module, &pkg.repo, &target_rel, None);
                    self.store.upsert_node(&target_module)?;
                    // If we synthesized the path (no `main` field), explicitly connect it to the
                    // target repo node so it's not orphaned. Walker won't create this node for us
                    // because package.json isn't a .ts/.py/.go file.
                    if pkg.main_relative.is_none() {
                        let target_repo_node = Node::new(NodeKind::Repo, &pkg.repo, "", None);
                        self.store.upsert_edge(&Edge {
                            from: target_repo_node.id,
                            to: target_module.id.clone(),
                            kind: EdgeKind::Contains,
                        })?;
                        // Don't bump stats.edges — this synthesized Contains is a side effect of
                        // the Imports edge we're creating, not a primary traversal edge.
                    }
                    self.store.upsert_edge(&Edge {
                        from: module_node.id.clone(),
                        to: target_module.id,
                        kind: EdgeKind::Imports,
                    })?;
                    stats.edges += 1;
                }
                // Otherwise: external dep not in our workspace, skip silently.
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

//! `repolayer view --out <dir>` — export the four `.repolayer/` indices to a
//! self-contained HTML viewer (Cytoscape.js based).
//!
//! Output layout:
//!   <out>/index.html              — landing page with stats and links
//!   <out>/graph.html              — main cross-repo graph (Repo + Module level)
//!   <out>/deps.html               — file-level dependency graph (per repo)
//!   <out>/data/graph.json         — main graph elements (cytoscape format)
//!   <out>/data/deps.json          — deps graph elements grouped by repo
//!   <out>/data/repos.json         — list of indexed repos with stats
//!   <out>/data/outline/<repo>/<sha>.json
//!                                 — per-file Declaration JSON, lazy-loaded by
//!                                   the side panel when a Module node is
//!                                   clicked. `sha` is sha256(repo+path)[..16]
//!                                   to keep filenames flat.

use anyhow::{bail, Context, Result};
use rusqlite::Connection;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

const GRAPH_HTML: &str = include_str!("../../assets/view/graph.html");
const DEPS_HTML: &str = include_str!("../../assets/view/deps.html");
const INDEX_HTML: &str = include_str!("../../assets/view/index.html");

pub async fn run(out: PathBuf, repo_filter: Option<String>) -> Result<()> {
    let workspace = std::env::current_dir()?;
    let repolayer_dir = workspace.join(".repolayer");
    if !repolayer_dir.join("index.db").exists() {
        bail!(
            "no index found at {} — run `repolayer build` first",
            repolayer_dir.join("index.db").display()
        );
    }

    // Load repolayer.yml to map (repo name → root path) so we can normalise
    // outline.db's absolute paths to the relative paths used by the graph.
    let cfg_path = workspace.join("repolayer.yml");
    let repo_roots: HashMap<String, PathBuf> = if cfg_path.exists() {
        let cfg = crate::config::Config::from_path(&cfg_path)?;
        cfg.repos
            .iter()
            .filter_map(|r| {
                let name = r.name.clone().or_else(|| {
                    r.path
                        .file_name()
                        .map(|s| s.to_string_lossy().into_owned())
                })?;
                Some((name, r.path.clone()))
            })
            .collect()
    } else {
        HashMap::new()
    };

    fs::create_dir_all(&out).with_context(|| format!("creating {}", out.display()))?;
    let data_dir = out.join("data");
    fs::create_dir_all(&data_dir)?;
    let outline_root = data_dir.join("outline");
    fs::create_dir_all(&outline_root)?;

    // Outline first: it builds the (repo, path) → outline_key index used by
    // the main graph to decide which Module nodes are clickable.
    let (outline_stats, outline_index) = export_outlines(
        &repolayer_dir.join("outline.db"),
        &outline_root,
        repo_filter.as_deref(),
        &repo_roots,
    )?;
    eprintln!("outline files exported: {}", outline_stats);

    let stats = export_main_graph(
        &repolayer_dir.join("index.db"),
        &data_dir,
        repo_filter.as_deref(),
        &outline_index,
    )?;
    eprintln!(
        "graph: overview {}n/{}e, {} repo subgraphs (max {}n)",
        stats.overview_nodes, stats.overview_edges, stats.repo_count, stats.max_repo_nodes
    );

    let dep_stats = export_deps_graph(
        &repolayer_dir.join("deps.db"),
        &data_dir,
        repo_filter.as_deref(),
    )?;
    eprintln!(
        "deps.json: {} files, {} edges across {} repos",
        dep_stats.nodes, dep_stats.edges, dep_stats.repos
    );

    write_repos_json(&data_dir.join("repos.json"), &stats, &dep_stats, outline_stats)?;

    fs::write(out.join("graph.html"), GRAPH_HTML)?;
    fs::write(out.join("deps.html"), DEPS_HTML)?;
    fs::write(out.join("index.html"), INDEX_HTML)?;

    eprintln!("\nopen file://{}/index.html", out.display());
    Ok(())
}

#[derive(Default, Serialize)]
struct GraphStats {
    overview_nodes: usize,
    overview_edges: usize,
    repo_count: usize,
    max_repo_nodes: usize,
    repos: Vec<String>,
}

#[derive(Default, Serialize)]
struct DepStats {
    nodes: usize,
    edges: usize,
    repos: usize,
    per_repo: HashMap<String, (usize, usize)>,
}


/// Two-tier export so the browser never has to load 18MB up front:
///
///   data/overview.json     —— all Repo nodes + edges aggregated to repo→repo
///                             (43 nodes, ~80 edges; few KB; instant first paint)
///   data/repo/<repo>.json  —— Module + IdlService nodes for ONE repo, plus
///                             edges with at least one endpoint in that repo.
///                             Counterpart endpoints in other repos appear as
///                             stub "external" nodes so cross-repo edges show.
///
/// The HTML loads overview.json on page open and lazy-loads repo subgraphs on
/// double-click.
fn export_main_graph(
    db: &Path,
    data_dir: &Path,
    repo_filter: Option<&str>,
    outline_index: &HashMap<(String, String), String>,
) -> Result<GraphStats> {
    let conn = Connection::open(db)?;

    // Read all Repo + Module + IdlService nodes once.
    let mut stmt = conn.prepare(
        "SELECT id, kind, repo, path, symbol
         FROM nodes
         WHERE kind IN ('repo', 'module', 'idlservice')",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut node_meta: HashMap<String, (String, String, String, Option<String>)> = HashMap::new();
    let mut repo_node_id: HashMap<String, String> = HashMap::new();
    let mut repos_set: HashSet<String> = HashSet::new();
    for (id, kind, repo, path, symbol) in &rows {
        if let Some(f) = repo_filter {
            if repo != f {
                continue;
            }
        }
        node_meta.insert(
            id.clone(),
            (kind.clone(), repo.clone(), path.clone(), symbol.clone()),
        );
        repos_set.insert(repo.clone());
        if kind == "repo" {
            repo_node_id.insert(repo.clone(), id.clone());
        }
    }

    // IdlMethod (~83k) → owner IdlService remap (same repo+path).
    let mut idl_method_to_service: HashMap<String, String> = HashMap::new();
    {
        let mut svc_by_loc: HashMap<(String, String), String> = HashMap::new();
        for (id, (kind, repo, path, _)) in &node_meta {
            if kind == "idlservice" {
                svc_by_loc.insert((repo.clone(), path.clone()), id.clone());
            }
        }
        let mut mstmt = conn.prepare(
            "SELECT id, repo, path FROM nodes WHERE kind = 'idlmethod'",
        )?;
        let mrows = mstmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (mid, repo, path) in mrows {
            if let Some(svc_id) = svc_by_loc.get(&(repo, path)) {
                idl_method_to_service.insert(mid, svc_id.clone());
            }
        }
    }

    // Need a quick (id) → repo lookup that ALSO covers IdlMethod ids (otherwise
    // we'd lose the cross-repo edges that pass through methods). Build it from
    // the full node table.
    let mut id_to_repo: HashMap<String, String> = HashMap::new();
    {
        let mut stmt = conn.prepare("SELECT id, repo FROM nodes")?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (id, repo) in rows {
            id_to_repo.insert(id, repo);
        }
    }

    // Read all edges once. Drop `contains`. Apply IdlMethod→IdlService remap
    // for endpoint resolution but keep original repo via id_to_repo for cross-
    // repo classification.
    let mut estmt = conn.prepare("SELECT from_id, to_id, kind, confidence FROM edges")?;
    let erows = estmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, f32>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    // Aggregate cross-repo edges by (src_repo, dst_repo, kind) for overview.
    let mut overview_edge_counts: HashMap<(String, String, String), usize> = HashMap::new();
    // Per-repo subgraph edges (after IdlMethod→IdlService folding).
    type FoldedEdge = (String, String, String, f32);
    let mut per_repo_edges: HashMap<String, Vec<FoldedEdge>> = HashMap::new();

    let mut edge_seen: HashSet<(String, String, String)> = HashSet::new();
    for (from, to, kind, conf) in erows {
        if kind == "contains" {
            continue;
        }
        let from_folded = idl_method_to_service.get(&from).cloned().unwrap_or(from);
        let to_folded = idl_method_to_service.get(&to).cloned().unwrap_or(to);
        let key = (from_folded.clone(), to_folded.clone(), kind.clone());
        if !edge_seen.insert(key) {
            continue;
        }
        let Some(repo_a) = id_to_repo.get(&from_folded) else { continue; };
        let Some(repo_b) = id_to_repo.get(&to_folded) else { continue; };
        if let Some(f) = repo_filter {
            if repo_a != f && repo_b != f {
                continue;
            }
        }
        if repo_a != repo_b {
            *overview_edge_counts
                .entry((repo_a.clone(), repo_b.clone(), kind.clone()))
                .or_insert(0) += 1;
        }
        let edge = (from_folded.clone(), to_folded.clone(), kind.clone(), conf);
        // Both endpoints' repo subgraphs need this edge so a cross-repo edge
        // shows up no matter which side the user is looking at.
        per_repo_edges.entry(repo_a.clone()).or_default().push(edge.clone());
        if repo_b != repo_a {
            per_repo_edges.entry(repo_b.clone()).or_default().push(edge);
        }
    }

    // ---- write overview.json ----
    // Layout: rank repos by total degree. Top hubs go in the centre, the rest
    // are arranged on concentric rings. This matches the IDL-hub topology
    // typical of a backend monorepo set (rpc_idl / http_idl in the middle,
    // application repos around them).
    let mut degree: HashMap<String, usize> = HashMap::new();
    for ((sr, dr, _), n) in &overview_edge_counts {
        *degree.entry(sr.clone()).or_insert(0) += n;
        *degree.entry(dr.clone()).or_insert(0) += n;
    }
    let mut sorted_repos: Vec<String> = repos_set.iter().cloned().collect();
    sorted_repos.sort_by(|a, b| {
        degree.get(b).copied().unwrap_or(0)
            .cmp(&degree.get(a).copied().unwrap_or(0))
            .then_with(|| a.cmp(b))
    });

    let n = sorted_repos.len();
    // Center: top 2 hubs (or 1 if very few repos). Outer rings: rest.
    let n_hubs = if n > 6 { 2 } else { 1.min(n) };
    let outer_count = n.saturating_sub(n_hubs);
    let outer_pitch: f32 = 90.0;          // circumference spacing per node
    let outer_radius = ((outer_count as f32 * outer_pitch) / (2.0 * std::f32::consts::PI)).max(400.0);

    let overview_node_values: Vec<serde_json::Value> = sorted_repos
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let (x, y) = if i < n_hubs {
                // Hubs: stack vertically near origin
                let dy = if n_hubs > 1 {
                    (i as f32 - (n_hubs as f32 - 1.0) / 2.0) * 80.0
                } else {
                    0.0
                };
                (0.0f32, dy)
            } else {
                let j = i - n_hubs;
                let theta = (j as f32 / outer_count.max(1) as f32) * 2.0 * std::f32::consts::PI;
                (outer_radius * theta.cos(), outer_radius * theta.sin())
            };
            let id = repo_node_id.get(r).cloned().unwrap_or_else(|| format!("repo:{r}"));
            serde_json::json!({
                "data": {
                    "id": id, "label": r, "kind": "repo", "repo": r,
                    "degree": degree.get(r).copied().unwrap_or(0),
                    "is_hub": i < n_hubs,
                },
                "position": { "x": x, "y": y },
            })
        })
        .collect();

    let overview_edge_values: Vec<serde_json::Value> = overview_edge_counts
        .iter()
        .filter_map(|((sr, dr, kind), n)| {
            let s = repo_node_id.get(sr)?.clone();
            let t = repo_node_id.get(dr)?.clone();
            Some(serde_json::json!({
                "data": {
                    "id": format!("{}__{}__{}", sr, dr, kind),
                    "source": s,
                    "target": t,
                    "kind": kind,
                    "count": n,
                    "cross_repo": true,
                }
            }))
        })
        .collect();

    let overview_path = data_dir.join("overview.json");
    fs::write(
        &overview_path,
        serde_json::to_string(&serde_json::json!({
            "nodes": overview_node_values,
            "edges": overview_edge_values,
        }))?,
    )?;

    // ---- write per-repo subgraphs ----
    let repo_dir = data_dir.join("repo");
    fs::create_dir_all(&repo_dir)?;
    let mut max_repo_nodes = 0usize;
    for repo in &sorted_repos {
        // Collect nodes for this repo (Module + IdlService) plus stub external
        // endpoints reachable via per_repo_edges.
        let local_ids: HashSet<String> = node_meta
            .iter()
            .filter(|(_, (kind, r, _, _))| r == repo && kind != "repo")
            .map(|(id, _)| id.clone())
            .collect();
        let edges = per_repo_edges.get(repo).cloned().unwrap_or_default();
        let mut external_ids: HashSet<String> = HashSet::new();
        for (from, to, _, _) in &edges {
            if !local_ids.contains(from) {
                external_ids.insert(from.clone());
            }
            if !local_ids.contains(to) {
                external_ids.insert(to.clone());
            }
        }

        // Layout: local nodes in a grid; external nodes pinned to the right.
        let mut local_sorted: Vec<&String> = local_ids.iter().collect();
        local_sorted.sort();
        let inner_cols = (local_sorted.len() as f32).sqrt().ceil().max(1.0) as usize;
        let inner_pitch: f32 = 36.0;

        let mut nodes_json: Vec<serde_json::Value> = Vec::new();
        for (i, id) in local_sorted.iter().enumerate() {
            let Some((kind, r, path, symbol)) = node_meta.get(*id) else { continue; };
            let x = (i % inner_cols) as f32 * inner_pitch;
            let y = (i / inner_cols) as f32 * inner_pitch;
            let label = if kind == "module" {
                Path::new(path)
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.clone())
            } else {
                symbol.clone().unwrap_or_else(|| path.clone())
            };
            let outline = if kind == "module" {
                match outline_index.get(&(r.clone(), path.clone())) {
                    Some(k) => serde_json::Value::String(k.clone()),
                    None => serde_json::Value::Null,
                }
            } else {
                serde_json::Value::Null
            };
            nodes_json.push(serde_json::json!({
                "data": {
                    "id": id,
                    "label": label,
                    "kind": kind,
                    "repo": r,
                    "path": path,
                    "symbol": symbol,
                    "outline_key": outline,
                },
                "position": { "x": x, "y": y },
            }));
        }
        // Pin externals into a right-side column, grouped by their repo.
        let mut ext_by_repo: HashMap<String, Vec<String>> = HashMap::new();
        for id in &external_ids {
            if let Some(r) = id_to_repo.get(id) {
                ext_by_repo.entry(r.clone()).or_default().push(id.clone());
            }
        }
        let inner_w = inner_cols as f32 * inner_pitch;
        let ext_x_base = inner_w + 200.0;
        let mut ext_y = 0f32;
        for (r, ids) in &ext_by_repo {
            for id in ids {
                let label = node_meta
                    .get(id)
                    .map(|(_kind, _r, path, sym)| {
                        sym.clone().unwrap_or_else(|| {
                            Path::new(path)
                                .file_name()
                                .map(|s| s.to_string_lossy().into_owned())
                                .unwrap_or_else(|| path.clone())
                        })
                    })
                    .unwrap_or_else(|| id.clone());
                nodes_json.push(serde_json::json!({
                    "data": {
                        "id": id,
                        "label": format!("{}::{}", r, label),
                        "kind": node_meta.get(id).map(|m| m.0.as_str()).unwrap_or("module"),
                        "repo": r,
                        "external": true,
                    },
                    "position": { "x": ext_x_base, "y": ext_y },
                }));
                ext_y += 32.0;
            }
            ext_y += 16.0;
        }

        let edges_json: Vec<serde_json::Value> = edges
            .iter()
            .filter_map(|(from, to, kind, conf)| {
                let repo_a = id_to_repo.get(from)?;
                let repo_b = id_to_repo.get(to)?;
                let cross_repo = repo_a != repo_b;
                Some(serde_json::json!({
                    "data": {
                        "id": format!("{}__{}__{}", from, to, kind),
                        "source": from,
                        "target": to,
                        "kind": kind,
                        "confidence": conf,
                        "cross_repo": cross_repo,
                    }
                }))
            })
            .collect();

        max_repo_nodes = max_repo_nodes.max(nodes_json.len());
        let path = repo_dir.join(format!("{}.json", sanitize(repo)));
        fs::write(
            &path,
            serde_json::to_string(&serde_json::json!({
                "repo": repo,
                "nodes": nodes_json,
                "edges": edges_json,
            }))?,
        )?;
    }

    Ok(GraphStats {
        overview_nodes: overview_node_values.len(),
        overview_edges: overview_edge_values.len(),
        repo_count: sorted_repos.len(),
        max_repo_nodes,
        repos: sorted_repos,
    })
}

fn export_deps_graph(db: &Path, data_dir: &Path, repo_filter: Option<&str>) -> Result<DepStats> {
    let conn = Connection::open(db)?;
    let mut stmt = conn.prepare(
        "SELECT repo, from_path, to_path, edge_kind FROM forward_edges",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    type DepRepoBucket = (HashSet<String>, Vec<(String, String, String)>);
    let mut per_repo: HashMap<String, DepRepoBucket> = HashMap::new();
    for (repo, from, to, kind) in rows {
        if let Some(f) = repo_filter {
            if repo != f {
                continue;
            }
        }
        let entry = per_repo.entry(repo).or_default();
        entry.0.insert(from.clone());
        entry.0.insert(to.clone());
        entry.1.push((from, to, kind));
    }

    let mut stats = DepStats::default();
    let deps_dir = data_dir.join("deps");
    fs::create_dir_all(&deps_dir)?;
    for (repo, (files, edges)) in &per_repo {
        // Group files by their parent directory so visually adjacent nodes
        // come from the same folder. Each group lays out as an inner grid; the
        // groups themselves go in an outer grid sorted by group size desc.
        let mut by_dir: HashMap<String, Vec<String>> = HashMap::new();
        for p in files {
            let dir = Path::new(p)
                .parent()
                .map(|d| d.to_string_lossy().into_owned())
                .unwrap_or_default();
            by_dir.entry(dir).or_default().push(p.clone());
        }
        let mut groups: Vec<(String, Vec<String>)> = by_dir.into_iter().collect();
        groups.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(&b.0)));
        for (_, paths) in groups.iter_mut() {
            paths.sort();
        }

        // Outer grid for groups
        let n_groups = groups.len().max(1);
        let outer_cols = (n_groups as f32).sqrt().ceil() as usize;
        let inner_pitch: f32 = 26.0;
        // Estimate cell size by largest group so groups don't overlap.
        let largest = groups.iter().map(|(_, v)| v.len()).max().unwrap_or(1);
        let inner_cols_max = (largest as f32).sqrt().ceil() as usize;
        let cell_w = inner_cols_max as f32 * inner_pitch + 80.0;
        let cell_h = inner_cols_max as f32 * inner_pitch + 80.0;

        let mut positions: HashMap<String, (f32, f32)> = HashMap::new();
        for (gi, (_dir, paths)) in groups.iter().enumerate() {
            let ox = (gi % outer_cols) as f32 * cell_w;
            let oy = (gi / outer_cols) as f32 * cell_h;
            let inner_cols = (paths.len() as f32).sqrt().ceil().max(1.0) as usize;
            for (j, p) in paths.iter().enumerate() {
                let ix = (j % inner_cols) as f32 * inner_pitch;
                let iy = (j / inner_cols) as f32 * inner_pitch;
                positions.insert(p.clone(), (ox + ix, oy + iy));
            }
        }

        let nodes: Vec<serde_json::Value> = files
            .iter()
            .map(|p| {
                let (x, y) = positions.get(p).copied().unwrap_or((0.0, 0.0));
                serde_json::json!({
                    "data": {
                        "id": p,
                        "label": Path::new(p).file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| p.clone()),
                        "path": p,
                    },
                    "position": { "x": x, "y": y },
                })
            })
            .collect();
        let edge_vals: Vec<serde_json::Value> = edges
            .iter()
            .enumerate()
            .map(|(i, (f, t, k))| {
                serde_json::json!({
                    "data": {
                        "id": format!("e{}", i),
                        "source": f,
                        "target": t,
                        "kind": k,
                    }
                })
            })
            .collect();
        stats.nodes += nodes.len();
        stats.edges += edge_vals.len();
        stats.per_repo.insert(repo.clone(), (nodes.len(), edge_vals.len()));
        let path = deps_dir.join(format!("{}.json", sanitize(repo)));
        fs::write(
            &path,
            serde_json::to_string(&serde_json::json!({
                "nodes": nodes, "edges": edge_vals,
            }))?,
        )?;
    }
    stats.repos = per_repo.len();

    // Tiny manifest so deps.html can populate the repo dropdown without
    // pulling all per-repo JSONs.
    let manifest: Vec<serde_json::Value> = stats
        .per_repo
        .iter()
        .map(|(r, (n, e))| serde_json::json!({ "repo": r, "files": n, "edges": e }))
        .collect();
    fs::write(
        data_dir.join("deps_manifest.json"),
        serde_json::to_string(&manifest)?,
    )?;
    Ok(stats)
}

fn export_outlines(
    db: &Path,
    out_dir: &Path,
    repo_filter: Option<&str>,
    repo_roots: &HashMap<String, PathBuf>,
) -> Result<(usize, HashMap<(String, String), String>)> {
    let conn = Connection::open(db)?;
    let mut stmt = conn.prepare(
        "SELECT repo, path, language, line_count, parse_errors, declarations FROM files",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut count = 0usize;
    let mut index: HashMap<(String, String), String> = HashMap::new();
    for (repo, abs_path, language, line_count, parse_errors, decls_json) in rows {
        if let Some(f) = repo_filter {
            if repo != f {
                continue;
            }
        }
        // Strip the repo root prefix so path matches what graph nodes use.
        let rel_path = match repo_roots.get(&repo) {
            Some(root) => Path::new(&abs_path)
                .strip_prefix(root)
                .ok()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|| abs_path.clone()),
            None => abs_path.clone(),
        };
        let key = outline_key(&repo, &rel_path);
        index.insert((repo.clone(), rel_path.clone()), key.clone());
        let repo_subdir = out_dir.join(sanitize(&repo));
        fs::create_dir_all(&repo_subdir)?;
        let payload = serde_json::json!({
            "repo": repo,
            "path": rel_path,
            "language": language,
            "line_count": line_count,
            "parse_errors": parse_errors,
            "declarations": serde_json::from_str::<serde_json::Value>(&decls_json).unwrap_or(serde_json::Value::Null),
        });
        fs::write(repo_subdir.join(format!("{}.json", key)), payload.to_string())?;
        count += 1;
    }
    Ok((count, index))
}

fn write_repos_json(
    out: &Path,
    g: &GraphStats,
    d: &DepStats,
    outline_count: usize,
) -> Result<()> {
    let payload = serde_json::json!({
        "graph": {
            "overview_nodes": g.overview_nodes,
            "overview_edges": g.overview_edges,
            "repo_count": g.repo_count,
            "max_repo_nodes": g.max_repo_nodes,
            "repos": g.repos,
        },
        "deps": {
            "nodes": d.nodes,
            "edges": d.edges,
            "repos": d.repos,
            "per_repo": d.per_repo.iter().map(|(k, v)| {
                serde_json::json!({ "repo": k, "files": v.0, "edges": v.1 })
            }).collect::<Vec<_>>(),
        },
        "outline_files": outline_count,
    });
    fs::write(out, serde_json::to_string_pretty(&payload)?)?;
    Ok(())
}

fn outline_key(repo: &str, path: &str) -> String {
    let mut h = Sha256::new();
    h.update(repo.as_bytes());
    h.update(b"\0");
    h.update(path.as_bytes());
    let bytes = h.finalize();
    hex::encode(&bytes[..16])
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

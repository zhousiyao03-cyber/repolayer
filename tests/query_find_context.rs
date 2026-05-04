use repolayer::graph::model::*;
use repolayer::graph::store::Store;
use repolayer::query::find_context::find_context;
use tempfile::tempdir;

#[test]
fn find_context_returns_relevant_symbols() {
    let dir = tempdir().unwrap();
    let store = Store::open(&dir.path().join("index.db")).unwrap();
    let nodes = [
        ("repo_a", "src/auth.ts", "login"),
        ("repo_a", "src/auth.ts", "logout"),
        ("repo_a", "src/payment.ts", "charge"),
        ("repo_b", "src/redeem.ts", "redeemBenefit"),
    ];
    for (r, p, s) in &nodes {
        let n = Node::new(NodeKind::Function, r, p, Some(s));
        store.upsert_node(&n).unwrap();
    }

    let result = find_context(&store, "redeem benefit", 5000).unwrap();
    let symbols: Vec<_> = result
        .items
        .iter()
        .filter_map(|i| i.symbol.clone())
        .collect();
    assert!(
        symbols.iter().any(|s| s == "redeemBenefit"),
        "redeemBenefit should be in results"
    );
}

#[test]
fn find_context_respects_token_budget() {
    let dir = tempdir().unwrap();
    let store = Store::open(&dir.path().join("index.db")).unwrap();
    // Insert 100 symbols all with "test" in their name
    for i in 0..100 {
        let name = format!("test_func_{}", i);
        let n = Node::new(
            NodeKind::Function,
            "r",
            &format!("src/f{}.ts", i),
            Some(&name),
        );
        store.upsert_node(&n).unwrap();
    }
    let result = find_context(&store, "test", 200).unwrap();
    // 200 token budget at 80 tokens/item should fit ~2 items
    assert!(
        result.items.len() <= 3,
        "expected ≤3 items within 200 token budget, got {}",
        result.items.len()
    );
    assert!(
        result.total_tokens <= 250,
        "total_tokens {} should be near budget 200",
        result.total_tokens
    );
}

#[test]
fn find_context_returns_helpful_suggestion_when_empty() {
    let dir = tempdir().unwrap();
    let store = Store::open(&dir.path().join("index.db")).unwrap();
    let result = find_context(&store, "nothing matches xyzabc", 5000).unwrap();
    assert!(result.items.is_empty());
    assert!(
        result.suggestion.contains("No matches") || result.suggestion.contains("no matches"),
        "suggestion should mention no matches: {}",
        result.suggestion
    );
}

#[test]
fn find_context_dedups_by_node_id() {
    let dir = tempdir().unwrap();
    let store = Store::open(&dir.path().join("index.db")).unwrap();
    // Single node that contains both query tokens
    let n = Node::new(NodeKind::Function, "r", "src/auth.ts", Some("login_user"));
    store.upsert_node(&n).unwrap();
    let result = find_context(&store, "login user", 5000).unwrap();
    assert_eq!(
        result.items.len(),
        1,
        "node hit by multiple tokens should appear once, got {}",
        result.items.len()
    );
}

// --- Tests for new fields added in C-1 ---

#[test]
fn find_context_has_schema_version() {
    let dir = tempdir().unwrap();
    let store = Store::open(&dir.path().join("index.db")).unwrap();
    let n = Node::new(NodeKind::Function, "r", "src/foo.ts", Some("foo"));
    store.upsert_node(&n).unwrap();
    let result = find_context(&store, "foo", 5000).unwrap();
    assert_eq!(
        result.schema_version, "repolayer.find_context.v1",
        "schema_version must be stable"
    );
}

#[test]
fn find_context_items_have_match_source_and_confidence() {
    let dir = tempdir().unwrap();
    let store = Store::open(&dir.path().join("index.db")).unwrap();
    let n = Node::new(NodeKind::Function, "r", "src/auth.ts", Some("authenticateUser"));
    store.upsert_node(&n).unwrap();
    let result = find_context(&store, "authenticate user", 5000).unwrap();
    assert!(!result.items.is_empty(), "should have at least one result");
    let item = &result.items[0];
    assert_eq!(item.match_source, "substring", "substring is the only active path");
    assert!(
        item.confidence >= 0.0 && item.confidence <= 1.0,
        "confidence must be in [0, 1]"
    );
    assert!(item.estimated_tokens > 0, "estimated_tokens must be positive");
}

#[test]
fn find_context_cross_repo_edges_empty_when_no_cross_edges() {
    let dir = tempdir().unwrap();
    let store = Store::open(&dir.path().join("index.db")).unwrap();
    // Single isolated node, no edges
    let n = Node::new(NodeKind::Function, "repo_a", "src/handler.ts", Some("handleRequest"));
    store.upsert_node(&n).unwrap();
    let result = find_context(&store, "handle request", 5000).unwrap();
    assert!(!result.items.is_empty());
    assert!(
        result.items[0].cross_repo_edges.is_empty(),
        "no cross-repo edges expected for an isolated node"
    );
}

#[test]
fn find_context_cross_repo_edges_populated_for_cross_repo_imports() {
    let dir = tempdir().unwrap();
    let store = Store::open(&dir.path().join("index.db")).unwrap();

    // Use Function nodes with symbols so search_symbols_substring can find them.
    let src = Node::new(NodeKind::Function, "repo_a", "src/client.ts", Some("clientFetch"));
    let dst = Node::new(NodeKind::Function, "repo_b", "src/server.ts", Some("serverHandler"));
    store.upsert_node(&src).unwrap();
    store.upsert_node(&dst).unwrap();

    // Cross-repo Imports edge: repo_a → repo_b
    let edge = Edge {
        from: src.id.clone(),
        to: dst.id.clone(),
        kind: EdgeKind::Imports,
        confidence: 0.9,
    };
    store.upsert_edge(&edge).unwrap();

    let result = find_context(&store, "clientFetch", 5000).unwrap();
    let item = result
        .items
        .iter()
        .find(|i| i.repo == "repo_a")
        .expect("repo_a item should be present");

    assert_eq!(
        item.cross_repo_edges.len(),
        1,
        "should have one cross-repo edge"
    );
    let er = &item.cross_repo_edges[0];
    assert_eq!(er.target_repo, "repo_b");
    assert_eq!(er.target_path, "src/server.ts");
    assert!((er.confidence - 0.9).abs() < 1e-4);
}

#[test]
fn find_context_same_repo_edges_not_in_cross_repo() {
    let dir = tempdir().unwrap();
    let store = Store::open(&dir.path().join("index.db")).unwrap();

    // Two Function nodes in the SAME repo, both discoverable by substring search
    let src = Node::new(NodeKind::Function, "repo_a", "src/a.ts", Some("intraFuncAlpha"));
    let dst = Node::new(NodeKind::Function, "repo_a", "src/b.ts", Some("intraFuncBeta"));
    store.upsert_node(&src).unwrap();
    store.upsert_node(&dst).unwrap();

    let edge = Edge {
        from: src.id.clone(),
        to: dst.id.clone(),
        kind: EdgeKind::Imports,
        confidence: 1.0,
    };
    store.upsert_edge(&edge).unwrap();

    let result = find_context(&store, "intraFunc", 5000).unwrap();
    // Cross-repo edges list should be empty because both nodes are in repo_a
    for item in &result.items {
        if item.repo == "repo_a" {
            assert!(
                item.cross_repo_edges.is_empty(),
                "intra-repo edges must not appear in cross_repo_edges: item {:?}",
                item.symbol
            );
        }
    }
}

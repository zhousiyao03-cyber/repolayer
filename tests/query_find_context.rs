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

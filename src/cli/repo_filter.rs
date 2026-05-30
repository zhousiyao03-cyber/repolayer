//! Validation + did-you-mean for `--repo <name>` flags.
//!
//! Both `query` and `search` accept a repo filter. When the user passes a
//! name that doesn't exist in the index, we want a deterministic, agent-
//! friendly error: an exact-failure line plus a short list of close matches
//! (so the agent can correct its own typo without a roundtrip to the user).

use anyhow::{bail, Result};

/// Confirm that `name` appears in the workspace's known repo set. Returns
/// `Err` with a "did-you-mean" listing when it doesn't — caller propagates
/// the error verbatim.
///
/// `known` is expected to be the deterministic, alphabetised output of
/// `SearchStore::list_repo_names` / `Store::list_repo_names`. We don't sort
/// here so the error preserves whatever order the caller chose.
pub fn require_repo<'a>(name: &str, known: &'a [String]) -> Result<&'a str> {
    if let Some(found) = known.iter().find(|r| r.as_str() == name) {
        return Ok(found.as_str());
    }
    let suggestions = closest_matches(name, known, 5);
    if suggestions.is_empty() {
        bail!(
            "unknown repo '{}'. The index has no repos at all — run `repolayer build` first.",
            name
        );
    }
    bail!(
        "unknown repo '{}'. Did you mean one of: {}? See repolayer.yml for the full list.",
        name,
        suggestions.join(", "),
    );
}

/// Pick up to `n` candidates from `known` that look most like `query`.
/// Combines: substring containment first, then a cheap edit-distance score.
/// We don't pull in a fuzzy-match crate — this only runs on user error.
fn closest_matches(query: &str, known: &[String], n: usize) -> Vec<String> {
    let q_lower = query.to_lowercase();
    let mut scored: Vec<(u32, &String)> = known
        .iter()
        .map(|name| {
            let name_lower = name.to_lowercase();
            let score = if name_lower == q_lower {
                0
            } else if name_lower.contains(&q_lower) || q_lower.contains(&name_lower) {
                1
            } else {
                2 + edit_distance(&q_lower, &name_lower) as u32
            };
            (score, name)
        })
        .collect();
    scored.sort_by_key(|(s, _)| *s);
    // Filter out wildly-different names — when the cheapest option is already
    // dist > query.len(), nothing in the list is plausibly a typo.
    let cutoff = (query.len() as u32).saturating_add(2);
    scored
        .into_iter()
        .take_while(|(s, _)| *s <= cutoff)
        .take(n)
        .map(|(_, name)| name.clone())
        .collect()
}

/// Standard Levenshtein distance, bytewise on ASCII (good enough for repo
/// names which are conventionally `[a-z0-9_]+`).
fn edit_distance(a: &str, b: &str) -> usize {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    let n = a_bytes.len();
    let m = b_bytes.len();
    if n == 0 {
        return m;
    }
    if m == 0 {
        return n;
    }
    let mut prev: Vec<usize> = (0..=m).collect();
    let mut curr: Vec<usize> = vec![0; m + 1];
    for i in 1..=n {
        curr[0] = i;
        for j in 1..=m {
            let cost = if a_bytes[i - 1] == b_bytes[j - 1] {
                0
            } else {
                1
            };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[m]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_match_passes() {
        let known = vec!["a".to_string(), "b".to_string()];
        assert_eq!(require_repo("a", &known).unwrap(), "a");
    }

    #[test]
    fn typo_suggests_closest() {
        let known = vec![
            "payment_gateway_api".to_string(),
            "order_service".to_string(),
            "user_profile".to_string(),
        ];
        let err = require_repo("payment_gatway_api", &known).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("payment_gateway_api"), "msg: {msg}");
    }

    #[test]
    fn substring_outranks_distant_edit() {
        let known = vec![
            "user_profile".to_string(),
            "totally_unrelated".to_string(),
        ];
        let err = require_repo("profile", &known).unwrap_err();
        let msg = err.to_string();
        // The substring match should appear before any distant-edit fallback.
        let prom_idx = msg.find("user_profile");
        let unrel_idx = msg.find("totally_unrelated");
        assert!(
            prom_idx.is_some(),
            "user_profile should be suggested: {msg}"
        );
        if let (Some(p), Some(u)) = (prom_idx, unrel_idx) {
            assert!(p < u, "substring match should rank first: {msg}");
        }
    }

    #[test]
    fn empty_known_set_errors_clearly() {
        let known: Vec<String> = vec![];
        let err = require_repo("anything", &known).unwrap_err();
        assert!(err.to_string().contains("no repos"), "{}", err);
    }
}

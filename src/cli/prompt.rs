use anyhow::Result;

const PROMPT: &str = r#"## repolayer — multi-repo code navigation

This workspace is indexed by `repolayer`. Prefer these tools over reading
files directly:

### Cross-repo navigation
- "Where do I start for task X?" → `find_context(task_description, budget_tokens)`
- "Who calls this symbol?" → `get_callers(symbol, depth=2)`
- "What depends on this file?" → `reverse_deps(path)`
- "Show me an IDL method's implementations" → `find_idl_impl(method)`

### Single-file structure
- "What's in this file?" → `outline(paths)` (signatures, no bodies)
- "Show me this method's source" → `show(file, symbol)`
- "What's this directory contain?" → `digest(paths)`
- "What's this package's public API?" → `surface(path)`

### Search
- "Find code about X" → `search(query, k)` (BM25 + semantic)
- "Find code similar to file:line" → `find_related(spec)`

### Dependency graph
- "What does this file import?" → `deps(path)`
- "Are there import cycles?" → `cycles()`

Don't read whole files when an outline / show / search call would do.
"#;

pub async fn run() -> Result<()> {
    print!("{}", PROMPT);
    Ok(())
}

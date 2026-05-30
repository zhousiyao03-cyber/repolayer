<!-- Thanks for contributing to repolayer! -->

## What

<!-- What does this PR change, and why? Link any related issue (#123). -->

## How

<!-- Notable implementation details, trade-offs, or decisions worth flagging for review. -->

## Checklist

- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --all-targets -- -D warnings` passes
- [ ] `cargo test` passes
- [ ] Added/updated tests for the change
- [ ] Updated docs (`README.md` / `CLAUDE.md`) if behavior or surface changed
- [ ] If `NodeKind`/`EdgeKind` serde tags or a store schema changed, bumped `meta.schema_version` + added a migration

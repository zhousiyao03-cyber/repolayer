//! Stable JSON schema identifiers used in MCP tool responses.
//! Bump on breaking changes.

pub const JSON_SCHEMA_OUTLINE: &str = "ast-outline.outline.v1";
pub const JSON_SCHEMA_SHOW: &str = "ast-outline.show.v1";
pub const JSON_SCHEMA_IMPLEMENTS: &str = "ast-outline.implements.v1";
pub const JSON_SCHEMA_SURFACE: &str = "ast-outline.surface.v1";
pub const JSON_SCHEMA_DEPS: &str = "ast-outline.deps.v1";
pub const JSON_SCHEMA_REVERSE_DEPS: &str = "ast-outline.reverse-deps.v1";
pub const JSON_SCHEMA_CYCLES: &str = "ast-outline.cycles.v1";
pub const JSON_SCHEMA_GRAPH: &str = "ast-outline.graph.v1";
pub const JSON_SCHEMA_DEPS_INDEX: &str = "ast-outline.deps-index.v1";

// repolayer-original (added in later plans):
pub const JSON_SCHEMA_FIND_CONTEXT: &str = "repolayer.find_context.v1";
pub const JSON_SCHEMA_GET_SYMBOL: &str = "repolayer.get_symbol.v1";
pub const JSON_SCHEMA_GET_CALLERS: &str = "repolayer.get_callers.v1";
pub const JSON_SCHEMA_GET_DEPENDENCIES: &str = "repolayer.get_dependencies.v1";
pub const JSON_SCHEMA_LIST_REPOS: &str = "repolayer.list_repos.v1";
pub const JSON_SCHEMA_FIND_IDL_IMPL: &str = "repolayer.find_idl_impl.v1";

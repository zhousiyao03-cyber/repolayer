//! Shared file-walk filtering used by every ast-outline subcommand.
//!
//! Two layers on top of `ignore::WalkBuilder`'s default `.gitignore` handling:
//!
//! 1. **`.ast-outline-ignore`** — a custom gitignore-syntax file that lets a
//!    repo exclude paths from ast-outline specifically without polluting
//!    `.gitignore`. Useful for things like generated fixtures that you want
//!    git-tracked but not analysed.
//! 2. **Hardcoded denylist** — directories almost no one wants ast-outline to
//!    walk into (build outputs, dependency caches, vendored deps). A safety
//!    net for repos that forget to gitignore these.
//!
//! Both are applied uniformly across `outline`, `digest`, `show`,
//! `implements`, and the new `search` / `find-related` / `index` commands.

use ignore::WalkBuilder;
use std::path::Path;

/// File basename **suffixes** we always skip — even if `.gitignore` doesn't
/// list them. These match against the file basename (case-sensitive).
///
/// Rationale: these files are virtually always auto-generated or carry no
/// semantic value for cross-repo agent queries, and they dominate the long-tail
/// of `>8k char` chunks that hit the embedding provider's token limit. Skipping them keeps
/// dense-search recall focused on hand-written code.
///
/// Important non-inclusions:
///   - `*.d.ts`: kept in. Some `.d.ts` are generated IDL stubs that frontends
///     actually call (e.g. `*_api.d.ts`); the bot relies on them to trace RPC
///     usage. Add per-repo `.ast-outline-ignore` if a specific repo has
///     useless `.d.ts`.
///   - `.proto` / `.thrift`: IDL sources — these are *primary* artefacts and
///     get a different code path (`adapters/idl/`).
pub const HARDCODED_IGNORE_FILE_SUFFIXES: &[&str] = &[
    // Protobuf / gRPC codegen (Go / C++ / Python)
    ".pb.go",
    "_pb.go",
    ".pb.cc",
    ".pb.h",
    "_pb2.py",
    "_pb2_grpc.py",
    // Go test files
    "_test.go",
    // JS / TS test files
    ".test.ts",
    ".test.tsx",
    ".test.js",
    ".test.jsx",
    ".spec.ts",
    ".spec.tsx",
    ".spec.js",
    ".spec.jsx",
    // Minified bundles (not human-readable)
    ".min.js",
    ".min.css",
    // Lepus / Lynx codegen output
    ".lepus.ts",
    ".lepus.tsx",
];

/// Directories we always skip — even if `.gitignore` doesn't list them.
///
/// Synced with the file-selection plan. New entries should be ones that:
///   - virtually never contain searchable user code
///   - are huge enough to slow indexing meaningfully
///   - have a stable, conventional name
pub const HARDCODED_IGNORE_DIRS: &[&str] = &[
    // VCS
    ".git",
    ".hg",
    ".svn",
    ".jj",
    // Python
    "__pycache__",
    ".venv",
    "venv",
    ".tox",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
    // JS/TS
    "node_modules",
    ".next",
    ".nuxt",
    ".turbo",
    ".parcel-cache",
    // Build outputs
    "dist",
    "build",
    "out",
    ".eggs",
    "target",
    // Other
    ".cache",
    ".gradle",
    ".idea",
    ".vscode",
    // Self
    ".ast-outline",
];

/// Wire `.ast-outline-ignore` into a `WalkBuilder`.
///
/// Call this on every walker that should observe ast-outline's per-repo
/// excludes. It's separate from `should_skip_path` because the `ignore` crate
/// can prune ignored directories before recursing into them — much faster
/// than visiting every entry and post-filtering.
pub fn add_filters(builder: &mut WalkBuilder) {
    builder.add_custom_ignore_filename(".ast-outline-ignore");
}

/// Return `true` if any component of `path` (relative to `repo_root`) matches
/// the hardcoded denylist. Used as a post-filter — the `ignore` crate handles
/// `.gitignore` and `.ast-outline-ignore` for us, but the denylist is our
/// belt-and-suspenders.
///
/// Components are compared case-sensitively; directory names like
/// `node_modules` are conventionally lower-case on every platform.
pub fn should_skip_path(path: &Path, repo_root: &Path) -> bool {
    let Ok(rel) = path.strip_prefix(repo_root) else {
        return false;
    };
    // Directory denylist: applies anywhere in the path.
    if rel.components().any(|c| {
        let s = c.as_os_str().to_string_lossy();
        HARDCODED_IGNORE_DIRS.iter().any(|d| *d == s)
    }) {
        return true;
    }
    // File suffix denylist: applies to the basename only.
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        if HARDCODED_IGNORE_FILE_SUFFIXES
            .iter()
            .any(|sfx| name.ends_with(sfx))
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn skip_node_modules_anywhere() {
        let root = PathBuf::from("/r");
        assert!(should_skip_path(
            &root.join("node_modules/lodash/index.js"),
            &root
        ));
        assert!(should_skip_path(
            &root.join("packages/foo/node_modules/lib.js"),
            &root,
        ));
    }

    #[test]
    fn skip_target_dir() {
        let root = PathBuf::from("/r");
        assert!(should_skip_path(
            &root.join("target/debug/build/x.rs"),
            &root
        ));
    }

    #[test]
    fn skip_self_managed_index() {
        let root = PathBuf::from("/r");
        assert!(should_skip_path(
            &root.join(".ast-outline/index/meta.json"),
            &root
        ));
    }

    #[test]
    fn allow_normal_paths() {
        let root = PathBuf::from("/r");
        assert!(!should_skip_path(&root.join("src/main.rs"), &root));
        assert!(!should_skip_path(&root.join("docs/README.md"), &root));
    }

    #[test]
    fn skip_pb_go_stubs() {
        let root = PathBuf::from("/r");
        assert!(should_skip_path(
            &root.join("internal/model/service/example.pb.go"),
            &root
        ));
        assert!(should_skip_path(&root.join("foo_pb.go"), &root));
    }

    #[test]
    fn skip_test_files() {
        let root = PathBuf::from("/r");
        assert!(should_skip_path(&root.join("foo_test.go"), &root));
        assert!(should_skip_path(&root.join("src/Foo.test.ts"), &root));
        assert!(should_skip_path(&root.join("src/Foo.spec.tsx"), &root));
    }

    #[test]
    fn skip_minified_and_lepus() {
        let root = PathBuf::from("/r");
        assert!(should_skip_path(&root.join("dist/app.min.js"), &root));
        assert!(should_skip_path(
            &root.join("src/components/banner.lepus.ts"),
            &root
        ));
    }

    #[test]
    fn keep_dts_and_idl_sources() {
        // .d.ts files are NOT skipped — some are generated IDL stubs that
        // frontends import (e.g. `*_api.d.ts`).
        let root = PathBuf::from("/r");
        assert!(!should_skip_path(
            &root.join("src/api/example_service_api.d.ts"),
            &root
        ));
        // .proto / .thrift sources are primary IDL artefacts.
        assert!(!should_skip_path(&root.join("idl/service.proto"), &root));
        assert!(!should_skip_path(&root.join("idl/service.thrift"), &root));
    }

    #[test]
    fn allow_paths_outside_root() {
        let root = PathBuf::from("/r");
        // strip_prefix fails → not skipped (let caller decide).
        assert!(!should_skip_path(
            &PathBuf::from("/elsewhere/node_modules/x"),
            &root
        ));
    }
}

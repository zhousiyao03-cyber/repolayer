// Temporary stub — full impl adopted in Task B-6 (deps/graph.rs).
// Only the types consumed by extract.rs are defined here.

/// The kind of import statement extracted from source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportKind {
    /// Rust `use foo::Bar`
    Use,
    /// Rust `mod foo;`
    Mod,
    /// Python `from foo import Bar`
    From,
    /// Bare import: `import x` (Python / Go / Kotlin / Java)
    Bare,
    /// TS/JS `import { Foo } from 'x'`
    NamedFrom,
    /// `import * as x from 'x'` or Python `from x import *`
    StarFrom,
    /// Java/C# `import static ...`
    Static,
    /// Kotlin/C# alias `import X as Y` / `using A = B`
    Alias,
    /// Glob: `use foo::*` or `import a.b._`
    Glob,
}

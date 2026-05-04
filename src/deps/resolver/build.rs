// Temporary stub — full impl adopted in Task B-8 (deps/resolver/ subsystem).
// Only the `Lang` enum consumed by extract.rs is defined here.

/// Source language, used to dispatch extract.rs extractors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Rust,
    Python,
    TypeScript,
    Tsx,
    JavaScript,
    Scala,
    Java,
    Kotlin,
    CSharp,
    Go,
    Other,
}

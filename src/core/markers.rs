//! Marker post-processing: derives `native_kind`, `modifiers`, `deprecated`
//! fields on Declarations from the language-native conventions captured
//! by adapters in `attrs`/`signature`.
//!
//! Plan A ships a no-op stub so the dispatcher compiles. The full
//! implementation lands in Plan A Task 12.

use crate::core::declaration::Declaration;

pub fn populate_markers(_decls: &mut [Declaration], _language: &'static str) {
    // No-op stub. Replaced in Task 12.
}

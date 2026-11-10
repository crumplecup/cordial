//! Typed declaration facts for source and Kind edges.

mod native;
mod target;
pub use target::{ErrorEdgeTarget, ErrorEdgeTargetKind};

/// Which relationship a field on an error declaration represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ErrorEdgeField {
    /// A nested cause exposed through `Error::source`.
    Source,
    /// The error's category or variant payload.
    Kind,
}

/// The syntactic storage shape of a `source` or `kind` field.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ErrorEdgeStorage {
    /// A value stored directly in the field.
    Owned,
    /// A value stored behind `Box`.
    Boxed,
    /// A reference with an explicit lifetime.
    Borrowed {
        /// Lifetime named on the reference.
        lifetime: String,
    },
}

/// Parsed edge facts from the shared source tree; the path is not yet resolved.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    derive_getters::Getters,
    derive_setters::Setters,
)]
#[setters(generate = false, borrow_self)]
pub struct ErrorEdgeEvidence {
    #[getter(copy)]
    field: ErrorEdgeField,
    storage: ErrorEdgeStorage,
    /// Path segments for the stored value, before import or alias resolution.
    type_path: Vec<String>,
    /// Exact spelling of the declared type for diagnostics.
    type_text: String,
    /// Unique, private top-level declaration candidate in the crate target.
    #[setters(generate, rename = "set_target", strip_option)]
    target: Option<ErrorEdgeTarget>,
}

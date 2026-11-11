//! Where a function item sits in source, and the calls that leave its crate.
//!
//! A [`SemanticItemId`](super::SemanticItemId) names a definition only inside one
//! analysis session. A call from one workspace crate into another is recorded in
//! the caller's graph and matched against the callee's graph later, possibly in
//! another session (a cached IR beside a fresh one). The two sides agree on the
//! item's source location, which depends on nothing but the source.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Node attribute holding the function's [`FunctionLocation`].
pub const ATTR_FUNCTION_LOCATION: &str = "function_location";

/// Node attribute holding the calls a function makes into other crates.
pub const ATTR_EXTERNAL_CALLS: &str = "semantic_external_calls";

/// Node attribute holding the trait method a function implements.
pub const ATTR_TRAIT_IMPLEMENTATION: &str = "trait_implementation";

/// The file and byte offset where a function item starts, attributes included.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    derive_getters::Getters,
    derive_new::new,
)]
#[new(visibility = "pub(crate)")]
pub struct FunctionLocation {
    file: PathBuf,
    offset: u32,
}

/// A resolved call to a function that lives in another crate of the workspace.
///
/// `context` names the cfg context that resolved it; `None` is the ordinary
/// build. The callee is matched to a node of its own crate by location.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters, derive_new::new,
)]
#[new(visibility = "pub(crate)")]
pub struct ExternalCall {
    callee: FunctionLocation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    context: Option<super::SemanticContextKey>,
}

/// A function that implements a trait method.
///
/// A call through a trait names the trait's declaration, not the implementation
/// that runs, so a reader that follows calls treats a call to the declaration as
/// a call to every implementer. `declaration` is `None` when the trait is not
/// the workspace's: its callers are not all visible.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters, derive_new::new,
)]
#[new(visibility = "pub(crate)")]
pub struct TraitImplementation {
    declaration: Option<FunctionLocation>,
}

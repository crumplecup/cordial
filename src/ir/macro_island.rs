//! Functions written inside an item-position macro invocation.
//!
//! The invocation's tokens are read as items, once, from the database's syntax;
//! the functions found there become graph nodes whose calls are resolved in the
//! invocation's scope. They are gated by syntax, because a macro may emit them
//! only in a verifier's build.

use serde::{Deserialize, Serialize};

/// Node attribute marking a function read from a macro invocation's tokens.
pub const ATTR_MACRO_ISLAND: &str = "macro_island";

/// A function found inside a macro invocation.
///
/// `gates` are the cfg atoms on the function and its enclosing items and the
/// first segment of each attribute path (`kani` for `#[kani::proof]`): names
/// that, when a crate's gate configuration contains one, mark the function as
/// existing only in that verifier's build.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters, derive_new::new,
)]
#[new(visibility = "pub(crate)")]
pub struct MacroIsland {
    gates: Vec<String>,
}

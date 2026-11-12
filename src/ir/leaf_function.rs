//! What a function in a leaf crate calls, read off the library's definitions.
//!
//! An integration-test binary consumes the library and nothing consumes it, so it
//! has no place in the library's call graph. Its functions are still evidence:
//! each is recorded here under its source location, with what its calls resolved
//! to, and the library's definitions are matched by location as cross-crate calls
//! are.

use serde::{Deserialize, Serialize};

use super::{FunctionLocation, SemanticCallee};

/// Root attribute holding the leaf crates' [`LeafFunction`]s.
pub const ATTR_LEAF_FUNCTIONS: &str = "leaf_functions";

/// One function of a leaf crate and the definitions its calls resolved to.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters, derive_new::new,
)]
#[new(visibility = "pub(crate)")]
pub struct LeafFunction {
    location: FunctionLocation,
    callees: Vec<SemanticCallee>,
    /// Where the workspace functions among the callees are defined.
    called: Vec<FunctionLocation>,
}

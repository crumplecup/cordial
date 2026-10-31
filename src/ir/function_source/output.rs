//! Recognize the exact wrapper emitted by the return-lifting operation.

use serde::{Deserialize, Serialize};

mod compatibility;
mod native;
pub(super) use compatibility::{entry_contract, has_infallible_result_syntax, result_contract};
pub(super) use native::{
    entry_contract as native_entry_contract,
    has_infallible_result_syntax as native_core_result_syntax,
    result_contract as native_result_contract,
};

/// Syntactic entry signature; Cargo identity must be checked separately.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceEntryContract {
    /// A free `main` with no parameters and a unit return.
    UnitMain,
}

/// Explicit absolute standard Result syntax and its source type arguments.
///
/// This deliberately leaves aliases, relative paths, and generic error types
/// unresolved. `error_identity` is available only for an absolute nongeneric
/// standard type. Namespace and callee checks must precede any rewrite.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct SourceResultContract {
    success_syntax: String,
    error_syntax: String,
    error_identity: Option<String>,
}

impl SourceResultContract {
    /// Normalize known standard-library reexports without resolving new types.
    #[tracing::instrument(level = "trace", skip(self))]
    pub fn canonical_error_identity(&self) -> Option<&str> {
        self.error_identity
            .as_deref()
            .map(|identity| match identity {
                "::core::num::ParseIntError" => "::std::num::ParseIntError",
                "::std::convert::Infallible" => "::core::convert::Infallible",
                other => other,
            })
    }
}

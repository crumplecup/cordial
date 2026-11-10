//! Snapshot-bound function syntax retained by the shared source loader.

use serde::{Deserialize, Serialize};

mod calls;
mod native;
pub(crate) use native::signature_range as native_signature_range;
mod output;
mod returns;
mod success;
pub use calls::{FunctionCallSyntax, FunctionUnwrapSyntax, SourceCallUse};
pub use output::{SourceEntryContract, SourceResultContract};
pub use returns::{
    FunctionBoundary, FunctionNestedScope, FunctionReturn, FunctionReturnFlow, FunctionTail,
    NestedScopeKind, ReturnValue,
};
pub use success::FunctionSuccessExit;

/// Attribute carrying [`FunctionSourceSyntax`] on a source function node.
pub const ATTR_FUNCTION_SOURCE_SYNTAX: &str = "function_source_syntax";

/// Half-open byte range in the source snapshot named by its enclosing facts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct SourceByteRange {
    #[getter(copy)]
    start: usize,
    #[getter(copy)]
    end: usize,
}

impl From<std::ops::Range<usize>> for SourceByteRange {
    #[tracing::instrument(level = "trace", skip(range))]
    fn from(range: std::ops::Range<usize>) -> Self {
        Self {
            start: range.start,
            end: range.end,
        }
    }
}

/// Syntactic return contract, without interpreting or resolving the type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeclaredFunctionOutput {
    /// No return type is written; Rust supplies unit.
    ImplicitUnit,
    /// The exact type syntax following `->`.
    Explicit(String),
}

/// Declared syntax for a free function, collected from the shared parse.
///
/// These are source facts, not inferred types or proof of a Result contract.
/// Missing facts on older IR snapshots mean discovery must run again. Body
/// ranges locate future edits; they do not establish return-path or call-graph
/// closure. Those obligations require additional collected evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct FunctionSourceSyntax {
    #[getter(copy)]
    source_digest: [u8; 32],
    signature: String,
    declared_output: DeclaredFunctionOutput,
    signature_range: SourceByteRange,
    body_range: SourceByteRange,
    /// Type range, or an empty insertion range for an implicit unit return.
    output_range: SourceByteRange,
    return_flow: FunctionReturnFlow,
    /// Exact generated wrapper shape, used only to avoid repeated lifting.
    #[getter(copy)]
    has_infallible_result_syntax: bool,
    /// Explicit standard Result declaration; aliases remain unresolved.
    result_contract: Option<SourceResultContract>,
    calls: Vec<FunctionCallSyntax>,
    unwraps: Vec<FunctionUnwrapSyntax>,
    /// Local bindings and imports can shadow a direct call's written path.
    has_call_shadowing: bool,
    /// Missing on older snapshots; names alone never establish an entry signature.
    #[serde(default)]
    entry_contract: Option<SourceEntryContract>,
    /// Uninterpreted method/residual operations; missing on older snapshots.
    #[serde(default)]
    opaque_calls: Option<Vec<SourceByteRange>>,
}

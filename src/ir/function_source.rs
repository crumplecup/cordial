//! Snapshot-bound function syntax retained by the shared source loader.

use serde::{Deserialize, Serialize};

mod calls;
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

impl FunctionSourceSyntax {
    #[tracing::instrument(level = "trace", skip(file))]
    pub(crate) fn collect(
        file: &crate::loader::SharedParsedFile,
        item_index: usize,
    ) -> Option<Self> {
        use syn::spanned::Spanned;

        // Select syntax from its carrier; never accept a separately supplied AST.
        let syn::Item::Fn(function) = file.syntax().items.get(item_index)? else {
            return None;
        };
        let signature_range = function.sig.span().byte_range();
        let declared_output = match &function.sig.output {
            syn::ReturnType::Default => DeclaredFunctionOutput::ImplicitUnit,
            syn::ReturnType::Type(_, ty) => {
                DeclaredFunctionOutput::Explicit(file.source()[ty.span().byte_range()].to_string())
            }
        };
        let output_range = match &function.sig.output {
            syn::ReturnType::Default => signature_range.end..signature_range.end,
            syn::ReturnType::Type(_, ty) => ty.span().byte_range(),
        };
        let (calls, unwraps, has_call_shadowing, opaque_calls) = calls::collect(function);
        Some(Self {
            source_digest: file.source_digest(),
            signature: file.source()[signature_range.clone()].to_string(),
            declared_output,
            signature_range: signature_range.into(),
            body_range: function.block.span().byte_range().into(),
            output_range: output_range.into(),
            return_flow: FunctionReturnFlow::collect(function),
            has_infallible_result_syntax: output::has_infallible_result_syntax(
                &function.sig.output,
            ),
            result_contract: output::result_contract(file, &function.sig.output),
            calls,
            unwraps,
            has_call_shadowing,
            entry_contract: output::entry_contract(function),
            opaque_calls: Some(opaque_calls),
        })
    }
}

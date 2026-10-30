//! Source call relationships persisted in the shared graph.

use super::{FunctionBoundary, NodeId, SourceByteRange};
use serde::{Deserialize, Serialize};

mod collect;

/// A value path, including uses outside the direct-call graph.
///
/// Written names are not declaration identities. Enrichment must resolve a use
/// before it can authorize a transform; ambiguous matches remain obligations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct SourcePathUse {
    range: SourceByteRange,
    segments: Vec<String>,
    #[getter(copy)]
    absolute: bool,
    /// Qualified-self or generic arguments require richer resolution.
    #[getter(copy)]
    qualified: bool,
    /// Owning top-level free function, identified within the enclosing snapshot.
    owner_signature: Option<SourceByteRange>,
    /// Only a path directly in callee position has a call range.
    call_range: Option<SourceByteRange>,
    /// Closures, async/const blocks, and nested items have separate contracts.
    #[getter(copy)]
    nested_scope: bool,
}

/// Complete written value-use inventory for the enclosing module snapshot.
///
/// Macro tokens and verbatim islands are retained as opaque ranges, never
/// interpreted as absence of references. This inventory is collected together
/// with namespace facts in one traversal of the shared tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct SourceUseInventory {
    paths: Vec<SourcePathUse>,
    opaque_ranges: Vec<SourceByteRange>,
}

/// Module-level evidence needed before interpreting source names.
pub const ATTR_SOURCE_MODULE_SCOPE: &str = "source_module_scope";
/// Enriched call relationships on a source function node.
pub const ATTR_FUNCTION_CALL_FLOW: &str = "function_call_flow";

/// Conservative namespace constraints collected from the shared file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct SourceModuleScope {
    #[getter(copy)]
    source_digest: [u8; 32],
    /// End of the shared snapshot, before which existing bytes are preserved.
    #[getter(copy)]
    insertion_offset: usize,
    /// All written identifiers, including local bindings, conservatively reserve names.
    identifiers: Vec<String>,
    #[getter(copy)]
    opaque: bool,
    #[getter(copy)]
    extern_crates: bool,
    /// Older snapshots lack this evidence and cannot establish use-site closure.
    #[serde(default)]
    uses: Option<SourceUseInventory>,
}

impl SourceModuleScope {
    #[tracing::instrument(level = "trace", skip(file))]
    pub(crate) fn collect(file: &crate::loader::SharedParsedFile) -> Self {
        collect::collect(file)
    }
}

/// Why a written call cannot identify a source function in this resolver.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CallResolutionFailure {
    /// Imports, macros, local items, or missing scope evidence may change names.
    OpaqueScope,
    /// A qualified, generic, indirect, or shadowed call needs richer evidence.
    UnsupportedCallee,
    /// No unique source declaration exists in the calling module.
    MissingOrAmbiguousTarget,
    /// The source or target has a contract this resolver does not interpret.
    Boundary(FunctionBoundary),
}

/// Exact declaration identity, or an explicit unresolved obligation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceCallResolution {
    /// A direct call to a source free function in the same module.
    Source {
        /// Identity in this crate graph.
        function: NodeId,
        /// Snapshot in which the declaration was resolved.
        source_digest: [u8; 32],
    },
    /// This call remains visible without authorizing a transform.
    Unresolved(CallResolutionFailure),
}

/// Resolution of one located call, without copying the callee's type facts.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters, derive_new::new,
)]
pub struct ResolvedFunctionCall {
    range: SourceByteRange,
    resolution: SourceCallResolution,
}

/// Enrichment output for the exact collected function snapshot.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters, derive_new::new,
)]
pub struct FunctionCallFlow {
    #[getter(copy)]
    source_digest: [u8; 32],
    /// Absolute standard paths are trusted only without namespace ambiguity.
    #[getter(copy)]
    namespace_trusted: bool,
    calls: Vec<ResolvedFunctionCall>,
}

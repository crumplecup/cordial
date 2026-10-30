//! Source call relationships persisted in the shared graph.

use serde::{Deserialize, Serialize};
use syn::visit::Visit;

use super::{FunctionBoundary, NodeId, SourceByteRange};

/// Module-level evidence needed before interpreting source names.
pub const ATTR_SOURCE_MODULE_SCOPE: &str = "source_module_scope";
/// Enriched call relationships on a source function node.
pub const ATTR_FUNCTION_CALL_FLOW: &str = "function_call_flow";

/// Conservative namespace constraints collected from the shared file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct SourceModuleScope {
    #[getter(copy)]
    opaque: bool,
    #[getter(copy)]
    extern_crates: bool,
}

impl SourceModuleScope {
    #[tracing::instrument(level = "trace", skip(file))]
    pub(crate) fn collect(file: &crate::loader::SharedParsedFile) -> Self {
        let mut scope = Self {
            opaque: false,
            extern_crates: false,
        };
        scope.visit_file(file.syntax());
        scope
    }
}

impl<'ast> Visit<'ast> for SourceModuleScope {
    #[tracing::instrument(level = "trace", skip(self, attribute))]
    fn visit_attribute(&mut self, attribute: &'ast syn::Attribute) {
        self.opaque |= !attribute.path().is_ident("doc");
    }

    #[tracing::instrument(level = "trace", skip(self, item))]
    fn visit_item(&mut self, item: &'ast syn::Item) {
        self.opaque |= matches!(
            item,
            syn::Item::Use(_)
                | syn::Item::ExternCrate(_)
                | syn::Item::Macro(_)
                | syn::Item::Verbatim(_)
        );
        self.extern_crates |= matches!(item, syn::Item::ExternCrate(_));
        syn::visit::visit_item(self, item);
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

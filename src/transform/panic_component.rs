//! Compose a fresh panic-to-error preview through known incoming callers.

use super::{
    ErrorDeclarationBlocker, FailurePropagationBlocker, FunctionEditPlan, ReturnLiftBlocker,
};
use crate::ir::NodeId;

mod callers;
mod plan;
pub use plan::plan_owned_panic_component;

/// Result of composing declarations, failure conversion, and transitive callers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanicComponentDecision {
    /// A coherent preview up to an explicitly selected observation boundary.
    Ready(PanicComponentPreview),
    /// The selected panic site no longer exists.
    NoCandidate,
    /// No partial preview is returned for an unsupported obligation.
    Refused(PanicComponentBlocker),
}

/// An obligation that prevents this component preview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanicComponentBlocker {
    /// No supported source-preserving declaration can be constructed.
    Declaration(ErrorDeclarationBlocker),
    /// A required signature cannot be lifted.
    ReturnContract {
        /// Function whose signature requires additional handling.
        function: NodeId,
        /// Unsatisfied return-contract obligation.
        reason: ReturnLiftBlocker,
    },
    /// A required call cannot be proven against its snapshot.
    CallEvidence {
        /// Function containing the unproven call.
        function: NodeId,
        /// Unsatisfied source-call obligation.
        reason: FailurePropagationBlocker,
    },
    /// The selected observation boundary is not an incoming caller of the leaf.
    BoundaryNotReached,
    /// A different known caller chain terminates outside the selected boundary.
    OpenCaller(NodeId),
    /// Multiple failure sources need a separate error-variant obligation.
    AdditionalPanicSites(NodeId),
    /// Located edits or graph relationships contradict their owners.
    InvalidEvidence,
}

/// A snapshot-bound transitive preview, not authorization for a complete write.
///
/// The boundary itself acquires Result; its consumers must observe that change.
/// Unknown uses and function values are not proven closed by incoming Calls
/// edges. This type deliberately exposes no disk writer or worklist claim.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters, derive_new::new)]
#[new(visibility = "pub(super)")]
pub struct PanicComponentPreview {
    plan: FunctionEditPlan,
    functions: Vec<NodeId>,
    #[getter(copy)]
    boundary: NodeId,
    error_names: super::GeneratedErrorNames,
}

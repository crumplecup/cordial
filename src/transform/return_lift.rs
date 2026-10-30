//! Lift successful returns without discovering syntax downstream of the IR.

use std::ops::ControlFlow;

pub use super::preview::{FunctionEdit as ReturnLiftEdit, FunctionEditPlan as ReturnLiftPlan};
use super::success_edits::edits_from_facts;

use crate::error::CordialResult;
use crate::ir::{FunctionBoundary, IrView, ItemKind, NodeId, NodeKind};
use crate::objects::SourceSpan;

/// Result of planning one isolated return operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReturnLiftDecision {
    /// Located edits, without permission to write an incomplete migration.
    Ready(ReturnLiftPlan),
    /// The exact generated wrapper is already present; no edits are needed.
    AlreadyLifted,
    /// No partial plan is returned when an obligation is unsupported.
    Refused(ReturnLiftBlocker),
}

/// Evidence that prevents this operation from being planned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReturnLiftBlocker {
    /// A source function, span, or its typed facts are missing.
    MissingFunctionEvidence,
    /// The function has a contract requiring a different operation.
    Boundary(FunctionBoundary),
    /// Ranges are inconsistent with their owning signature, body, or returns.
    InvalidRanges,
    /// A generated declaration belongs to another module or source snapshot.
    MismatchedErrorContract,
    /// Namespace resolution has not established the generated type's scope.
    UntrustedNamespace,
    /// Changing an existing Result error contract needs a separate operation.
    AlreadyFallible,
}

/// Plan `T` to `Result<T, Infallible>` using only collected IR facts.
///
/// This supplies the success adapter in isolation. It neither proves caller
/// closure nor selects an error architecture for a real failure source.
#[tracing::instrument(level = "debug", skip(ir, function), err(level = "warn"))]
#[hotpath::measure]
pub fn plan_infallible_return_lift(
    ir: &dyn IrView,
    function: NodeId,
) -> CordialResult<ReturnLiftDecision> {
    plan_return_lift(ir, function, "::core::convert::Infallible")
}

/// Lift success paths to a generated owned error contract in the same module.
///
/// The declaration is a snapshot-bound planned output, not an arbitrary type
/// string. This operation does not append it, rewrite failures, or adapt callers.
#[tracing::instrument(level = "debug", skip(ir, function, declaration), err(level = "warn"))]
#[hotpath::measure]
pub fn plan_owned_error_return_lift(
    ir: &dyn IrView,
    function: NodeId,
    declaration: &super::OwnedErrorDeclaration,
) -> CordialResult<ReturnLiftDecision> {
    if let Some(blocker) = super::generated_contract::validate_target(ir, function, declaration)? {
        return Ok(ReturnLiftDecision::Refused(blocker));
    }
    plan_return_lift(ir, function, declaration.names().parent())
}

#[tracing::instrument(level = "trace", skip(ir, function), err(level = "warn"))]
fn plan_return_lift(
    ir: &dyn IrView,
    function: NodeId,
    error_type: &str,
) -> CordialResult<ReturnLiftDecision> {
    let Some(node) = ir.node(function) else {
        return Ok(ReturnLiftDecision::Refused(
            ReturnLiftBlocker::MissingFunctionEvidence,
        ));
    };
    let Some(span) = node.weight().span().as_ref() else {
        return Ok(ReturnLiftDecision::Refused(
            ReturnLiftBlocker::MissingFunctionEvidence,
        ));
    };
    if node.kind() != &NodeKind::Item(ItemKind::Fn) {
        return Ok(ReturnLiftDecision::Refused(
            ReturnLiftBlocker::MissingFunctionEvidence,
        ));
    }
    let Some(facts) = ir.function_source_syntax(function)? else {
        return Ok(ReturnLiftDecision::Refused(
            ReturnLiftBlocker::MissingFunctionEvidence,
        ));
    };
    if facts.has_infallible_result_syntax() {
        return Ok(if error_type == "::core::convert::Infallible" {
            ReturnLiftDecision::AlreadyLifted
        } else {
            ReturnLiftDecision::Refused(ReturnLiftBlocker::AlreadyFallible)
        });
    }
    if error_type != "::core::convert::Infallible" && facts.result_contract().is_some() {
        return Ok(ReturnLiftDecision::Refused(
            ReturnLiftBlocker::AlreadyFallible,
        ));
    }
    if let Some(boundary) = facts.return_flow().boundaries().first() {
        return Ok(ReturnLiftDecision::Refused(ReturnLiftBlocker::Boundary(
            *boundary,
        )));
    }
    let edits = match edits_from_facts(&facts, error_type) {
        ControlFlow::Continue(edits) => edits,
        ControlFlow::Break(blocker) => return Ok(ReturnLiftDecision::Refused(blocker)),
    };
    Ok(ReturnLiftDecision::Ready(ReturnLiftPlan::new(
        function,
        span.file().to_path_buf(),
        facts.source_digest(),
        edits,
    )))
}

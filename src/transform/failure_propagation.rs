//! Propagate one resolved source error without changing a function's signature.

use super::preview::{FunctionEdit, FunctionEditPlan};
use crate::error::CordialResult;
use crate::ir::{
    CallResolutionFailure, EdgeKind, FunctionBoundary, FunctionSourceSyntax, FunctionUnwrapSyntax,
    IrView, ItemKind, NodeId, NodeKind, SourceCallResolution,
};
use crate::objects::SourceSpan;
use std::ops::ControlFlow;

/// Result of planning one failure site independently of component migration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailurePropagationDecision {
    /// Located edits for a proven Result receiver and matching error contract.
    Ready(FunctionEditPlan),
    /// No unwrap candidate exists at the selected index, including after reload.
    NoCandidate,
    /// No partial plan is published when an obligation is unresolved.
    Refused(FailurePropagationBlocker),
}

/// An unsupported obligation or inconsistent snapshot prevents propagation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailurePropagationBlocker {
    /// A source function, span, or collected facts are absent.
    MissingFunctionEvidence,
    /// Call enrichment has not supplied a checked namespace and target.
    MissingCallEvidence,
    /// An execution or declaration boundary requires a separate contract.
    Boundary(FunctionBoundary),
    /// The receiver cannot identify one source declaration.
    UnresolvedReceiver(CallResolutionFailure),
    /// Either declaration lacks an explicit absolute standard Result contract.
    NotStandardResult,
    /// Error identity is unknown, including aliases and generic error types.
    UnresolvedErrorType,
    /// This operation does not invent conversions between different errors.
    ErrorTypeMismatch,
    /// Ranges, graph relationships, or snapshot identities disagree.
    InvalidEvidence,
}

/// Replace one proven Result unwrap with propagation inside an existing Result.
///
/// This preview preserves the source error directly. It does not generate error
/// declarations, alter signatures, prove caller closure, or write source files.
#[tracing::instrument(level = "debug", skip(ir, function), err(level = "warn"))]
#[hotpath::measure]
pub fn plan_result_unwrap_propagation(
    ir: &dyn IrView,
    function: NodeId,
    site_index: usize,
) -> CordialResult<FailurePropagationDecision> {
    use FailurePropagationBlocker as Blocker;
    use FailurePropagationDecision::{NoCandidate, Ready, Refused};
    let Some(node) = ir
        .node(function)
        .filter(|node| node.kind() == &NodeKind::Item(ItemKind::Fn))
    else {
        return Ok(Refused(Blocker::MissingFunctionEvidence));
    };
    let (Some(span), Some(facts)) = (node.weight().span(), ir.function_source_syntax(function)?)
    else {
        return Ok(Refused(Blocker::MissingFunctionEvidence));
    };
    let Some(site) = facts.unwraps().get(site_index) else {
        return Ok(NoCandidate);
    };
    if let Some(boundary) = facts
        .return_flow()
        .boundaries()
        .iter()
        .find(|boundary| !matches!(boundary, FunctionBoundary::Visible | FunctionBoundary::Try))
    {
        return Ok(Refused(Blocker::Boundary(*boundary)));
    }
    let callee = match resolved_callee(ir, function, &facts, site)? {
        ControlFlow::Break(blocker) => return Ok(Refused(blocker)),
        ControlFlow::Continue(callee) => callee,
    };
    if let Some(blocker) = error_contract(&facts, &callee) {
        return Ok(Refused(blocker));
    }
    let Some(edits) = suffix_edits(&facts, site) else {
        return Ok(Refused(Blocker::InvalidEvidence));
    };
    Ok(Ready(FunctionEditPlan::new(
        function,
        span.file().to_path_buf(),
        facts.source_digest(),
        edits,
    )))
}

#[tracing::instrument(level = "trace", skip(ir, function, facts, site), err(level = "warn"))]
pub(super) fn resolved_callee(
    ir: &dyn IrView,
    function: NodeId,
    facts: &FunctionSourceSyntax,
    site: &FunctionUnwrapSyntax,
) -> CordialResult<ControlFlow<FailurePropagationBlocker, FunctionSourceSyntax>> {
    use FailurePropagationBlocker as Blocker;
    let Some(flow) = ir.function_call_flow(function)? else {
        return Ok(ControlFlow::Break(Blocker::MissingCallEvidence));
    };
    if flow.source_digest() != facts.source_digest()
        || flow.calls().len() != facts.calls().len()
        || !flow
            .calls()
            .iter()
            .zip(facts.calls())
            .all(|(resolved, call)| resolved.range() == call.range())
    {
        return Ok(ControlFlow::Break(Blocker::InvalidEvidence));
    }
    if !flow.namespace_trusted() {
        return Ok(ControlFlow::Break(Blocker::UnresolvedReceiver(
            CallResolutionFailure::OpaqueScope,
        )));
    }
    let mut matches = flow
        .calls()
        .iter()
        .filter(|call| call.range() == site.receiver());
    let Some(call) = matches.next() else {
        return Ok(ControlFlow::Break(Blocker::UnresolvedReceiver(
            CallResolutionFailure::UnsupportedCallee,
        )));
    };
    if matches.next().is_some() {
        return Ok(ControlFlow::Break(Blocker::InvalidEvidence));
    }
    let (target, digest) = match call.resolution() {
        SourceCallResolution::Source {
            function,
            source_digest,
        } => (*function, *source_digest),
        SourceCallResolution::Unresolved(reason) => {
            return Ok(ControlFlow::Break(Blocker::UnresolvedReceiver(*reason)));
        }
    };
    if !ir.children(function, EdgeKind::Calls).contains(&target)
        || !ir
            .node(target)
            .is_some_and(|node| node.kind() == &NodeKind::Item(ItemKind::Fn))
    {
        return Ok(ControlFlow::Break(Blocker::InvalidEvidence));
    }
    let (Some(callee), Some(callee_flow)) = (
        ir.function_source_syntax(target)?,
        ir.function_call_flow(target)?,
    ) else {
        return Ok(ControlFlow::Break(Blocker::MissingCallEvidence));
    };
    if digest != callee.source_digest()
        || digest != callee_flow.source_digest()
        || !callee_flow.namespace_trusted()
    {
        return Ok(ControlFlow::Break(Blocker::InvalidEvidence));
    }
    Ok(ControlFlow::Continue(callee))
}

#[tracing::instrument(level = "trace", skip(caller, callee))]
fn error_contract(
    caller: &FunctionSourceSyntax,
    callee: &FunctionSourceSyntax,
) -> Option<FailurePropagationBlocker> {
    use FailurePropagationBlocker as Blocker;
    let (Some(caller), Some(callee)) = (caller.result_contract(), callee.result_contract()) else {
        return Some(Blocker::NotStandardResult);
    };
    let (Some(caller), Some(callee)) = (caller.error_identity(), callee.error_identity()) else {
        return Some(Blocker::UnresolvedErrorType);
    };
    (caller != callee).then_some(Blocker::ErrorTypeMismatch)
}

#[tracing::instrument(level = "trace", skip(facts, site))]
fn suffix_edits(
    facts: &FunctionSourceSyntax,
    site: &FunctionUnwrapSyntax,
) -> Option<Vec<FunctionEdit>> {
    let body = facts.body_range();
    let receiver = site.receiver();
    if site.range().start() != receiver.start()
        || receiver.start() >= receiver.end()
        || body.start() >= site.range().start()
        || body.end() <= site.range().end()
        || site.suffix_tokens().len() != 4
    {
        return None;
    }
    let mut end = receiver.end();
    let mut edits = vec![FunctionEdit::new((end..end).into(), "?".to_owned())];
    for (token, length) in site.suffix_tokens().iter().zip([1, 6, 1, 1]) {
        if token.start() < end || token.end().checked_sub(token.start()) != Some(length) {
            return None;
        }
        end = token.end();
        edits.push(FunctionEdit::new(token.clone(), String::new()));
    }
    (end == site.range().end()).then_some(edits)
}

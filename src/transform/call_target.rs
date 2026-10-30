//! Validate a located call against its shared graph target and snapshot.

use super::FailurePropagationBlocker;
use crate::error::CordialResult;
use crate::ir::{
    CallResolutionFailure, EdgeKind, FunctionSourceSyntax, IrView, ItemKind, NodeId, NodeKind,
    SourceCallResolution,
};
use std::ops::ControlFlow;

#[tracing::instrument(level = "trace", skip(ir, function, facts, range), err(level = "warn"))]
pub(super) fn resolved_call(
    ir: &dyn IrView,
    function: NodeId,
    facts: &FunctionSourceSyntax,
    range: &crate::ir::SourceByteRange,
) -> CordialResult<ControlFlow<FailurePropagationBlocker, (NodeId, FunctionSourceSyntax)>> {
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
    let mut matches = flow.calls().iter().filter(|call| call.range() == range);
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
    Ok(ControlFlow::Continue((target, callee)))
}

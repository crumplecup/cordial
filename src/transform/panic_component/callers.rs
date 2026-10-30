//! Transitive caller relationships and their located propagation edits.

use std::collections::HashSet;
use std::ops::ControlFlow;

use super::PanicComponentBlocker as Blocker;
use crate::error::CordialResult;
use crate::ir::{EdgeKind, FunctionSourceSyntax, IrView, NodeId, SourceCallResolution};
use crate::transform::{FailurePropagationBlocker, FunctionEdit};

#[tracing::instrument(level = "trace", skip(ir, leaf, boundary))]
pub(super) fn collect(
    ir: &dyn IrView,
    leaf: NodeId,
    boundary: NodeId,
) -> ControlFlow<Blocker, HashSet<NodeId>> {
    let mut functions = HashSet::new();
    let mut pending = vec![leaf];
    while let Some(function) = pending.pop() {
        if !functions.insert(function) || function == boundary {
            continue;
        }
        let callers = ir.parents(function, EdgeKind::Calls);
        if callers.is_empty() {
            return ControlFlow::Break(Blocker::OpenCaller(function));
        }
        pending.extend(callers);
    }
    if !functions.contains(&boundary) {
        return ControlFlow::Break(Blocker::BoundaryNotReached);
    }
    ControlFlow::Continue(functions)
}

#[tracing::instrument(
    level = "trace",
    skip(ir, function, facts, functions),
    err(level = "warn")
)]
pub(super) fn propagation(
    ir: &dyn IrView,
    function: NodeId,
    facts: &FunctionSourceSyntax,
    functions: &HashSet<NodeId>,
) -> CordialResult<ControlFlow<Blocker, Vec<FunctionEdit>>> {
    let failure = |reason| ControlFlow::Break(Blocker::CallEvidence { function, reason });
    let Some(flow) = ir.function_call_flow(function)? else {
        return Ok(failure(FailurePropagationBlocker::MissingCallEvidence));
    };
    let mut edits = Vec::new();
    let mut targets = HashSet::new();
    for call in flow.calls() {
        if let SourceCallResolution::Source {
            function: target, ..
        } = call.resolution()
        {
            if !functions.contains(target) {
                continue;
            }
            match super::super::call_target::resolved_call(ir, function, facts, call.range())? {
                ControlFlow::Continue((resolved, _)) => {
                    targets.insert(resolved);
                }
                ControlFlow::Break(reason) => return Ok(failure(reason)),
            }
            let range = call.range();
            if facts.body_range().start() >= range.start()
                || range.start() >= range.end()
                || range.end() >= facts.body_range().end()
            {
                return Ok(ControlFlow::Break(Blocker::InvalidEvidence));
            }
            edits.push(FunctionEdit::new(
                (range.end()..range.end()).into(),
                "?".to_owned(),
            ));
        }
    }
    // Every adjacency used to discover this caller must have located evidence.
    if ir
        .children(function, EdgeKind::Calls)
        .iter()
        .any(|target| functions.contains(target) && !targets.contains(target))
    {
        return Ok(failure(FailurePropagationBlocker::InvalidEvidence));
    }
    Ok(ControlFlow::Continue(edits))
}

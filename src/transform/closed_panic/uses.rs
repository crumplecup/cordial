//! Join written references to resolved, located calls before claiming closure.

use std::collections::{HashMap, HashSet};
use std::ops::ControlFlow;

use super::super::PanicComponentPreview;
use super::ClosedPanicBlocker as Blocker;
use crate::error::CordialResult;
use crate::ir::{
    BasicQuery, EdgeKind, FileSnapshot, IrView, NodeId, NodeKind, SourceCallResolution,
    SourcePathUse,
};
use crate::objects::SourceSpan;

type CallSites = HashSet<(NodeId, usize, usize)>;

#[tracing::instrument(level = "trace", skip(ir, component), err(level = "warn"))]
pub(super) fn validate(
    ir: &dyn IrView,
    component: &PanicComponentPreview,
) -> CordialResult<ControlFlow<Blocker, Vec<FileSnapshot>>> {
    let mut names = HashMap::new();
    for function in component.functions() {
        let Some(name) = ir
            .node(*function)
            .and_then(|node| node.weight().name().clone())
        else {
            return Ok(ControlFlow::Break(Blocker::IncompleteUseEvidence(
                *function,
            )));
        };
        if names
            .insert(name.trim_start_matches("r#").to_owned(), *function)
            .is_some()
        {
            return Ok(ControlFlow::Break(Blocker::IncompleteUseEvidence(
                *function,
            )));
        }
    }
    let mut seen = CallSites::new();
    let mut inputs = Vec::new();
    for module in ir
        .nodes_matching(&BasicQuery::all_nodes())
        .into_iter()
        .filter(|node| node.kind() == &NodeKind::Module)
    {
        let Some(scope) = ir.source_module_scope(module.id())? else {
            return Ok(ControlFlow::Break(Blocker::MissingUseInventory(
                module.id(),
            )));
        };
        if scope.opaque() {
            return Ok(ControlFlow::Break(Blocker::UnresolvedScope(module.id())));
        }
        let Some(inventory) = scope.uses() else {
            return Ok(ControlFlow::Break(Blocker::MissingUseInventory(
                module.id(),
            )));
        };
        if let Some(range) = inventory.opaque_ranges().first() {
            return Ok(ControlFlow::Break(Blocker::OpaqueSyntax {
                module: module.id(),
                range: range.clone(),
            }));
        }
        for usage in inventory.paths() {
            let Some(target) = usage.segments().last().and_then(|name| names.get(name)) else {
                continue;
            };
            if !resolved_use(
                ir,
                module.id(),
                scope.source_digest(),
                usage,
                *target,
                component,
                &mut seen,
            )? {
                return Ok(ControlFlow::Break(Blocker::UnresolvedUse {
                    module: module.id(),
                    range: usage.range().clone(),
                }));
            }
        }
        let Some(span) = module.weight().span() else {
            return Ok(ControlFlow::Break(Blocker::IncompleteUseEvidence(
                module.id(),
            )));
        };
        inputs.push(FileSnapshot::new(
            span.file().to_path_buf(),
            scope.source_digest(),
        ));
    }
    if let Some(blocker) = coverage(ir, component, &seen)? {
        return Ok(ControlFlow::Break(blocker));
    }
    Ok(ControlFlow::Continue(inputs))
}

#[tracing::instrument(
    level = "trace",
    skip(ir, module, usage, target, component, seen),
    err(level = "warn")
)]
fn resolved_use(
    ir: &dyn IrView,
    module: NodeId,
    digest: [u8; 32],
    usage: &SourcePathUse,
    target: NodeId,
    component: &PanicComponentPreview,
    seen: &mut CallSites,
) -> CordialResult<bool> {
    let (Some(signature), Some(call)) = (usage.owner_signature(), usage.call_range()) else {
        return Ok(false);
    };
    if usage.nested_scope()
        || usage.absolute()
        || usage.qualified()
        || usage.segments().len() != 1
        || call.start() > usage.range().start()
        || usage.range().end() > call.end()
    {
        return Ok(false);
    }
    let mut owners = Vec::new();
    for owner in ir.children(module, EdgeKind::Contains) {
        if let Some(facts) = ir.function_source_syntax(owner)?
            && facts.source_digest() == digest
            && facts.signature_range() == signature
        {
            owners.push((owner, facts))
        }
    }
    let [(owner, facts)] = owners.as_slice() else {
        return Ok(false);
    };
    if !component.functions().contains(owner) {
        return Ok(false);
    }
    let ControlFlow::Continue((resolved, _)) =
        super::super::call_target::resolved_call(ir, *owner, facts, call)?
    else {
        return Ok(false);
    };
    if resolved != target {
        return Ok(false);
    }
    Ok(seen.insert((*owner, call.start(), call.end())))
}

#[tracing::instrument(level = "trace", skip(ir, component, seen), err(level = "warn"))]
fn coverage(
    ir: &dyn IrView,
    component: &PanicComponentPreview,
    seen: &CallSites,
) -> CordialResult<Option<Blocker>> {
    for function in component.functions() {
        let Some(flow) = ir.function_call_flow(*function)? else {
            return Ok(Some(Blocker::IncompleteUseEvidence(*function)));
        };
        for call in flow.calls() {
            if let SourceCallResolution::Source {
                function: target, ..
            } = call.resolution()
                && component.functions().contains(target)
                && !seen.contains(&(*function, call.range().start(), call.range().end()))
            {
                return Ok(Some(Blocker::IncompleteUseEvidence(*function)));
            }
        }
    }
    Ok(None)
}

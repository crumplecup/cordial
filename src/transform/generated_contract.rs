//! Bind planned declarations to consumers in the same shared source snapshot.

use crate::error::CordialResult;
use crate::ir::{EdgeKind, IrView, ItemKind, NodeId, NodeKind};
use crate::objects::SourceSpan;

use super::{OwnedErrorDeclaration, ReturnLiftBlocker};

#[tracing::instrument(level = "trace", skip(ir, function, declaration), err(level = "warn"))]
pub(super) fn validate_target(
    ir: &dyn IrView,
    function: NodeId,
    declaration: &OwnedErrorDeclaration,
) -> CordialResult<Option<ReturnLiftBlocker>> {
    use ReturnLiftBlocker as Blocker;
    let origin = declaration.plan();
    let parents = ir.parents(function, EdgeKind::Contains);
    let origin_parents = ir.parents(origin.function(), EdgeKind::Contains);
    if parents.len() != 1 || parents != origin_parents {
        return Ok(Some(Blocker::MismatchedErrorContract));
    }
    let Some(node) = ir
        .node(function)
        .filter(|node| node.kind() == &NodeKind::Item(ItemKind::Fn))
    else {
        return Ok(Some(Blocker::MissingFunctionEvidence));
    };
    let (Some(facts), Some(origin_facts), Some(scope)) = (
        ir.function_source_syntax(function)?,
        ir.function_source_syntax(origin.function())?,
        ir.source_module_scope(parents[0])?,
    ) else {
        return Ok(Some(Blocker::MissingFunctionEvidence));
    };
    if facts.source_digest() != origin.source_digest()
        || origin_facts.source_digest() != origin.source_digest()
        || scope.source_digest() != origin.source_digest()
        || !node
            .weight()
            .span()
            .as_ref()
            .is_some_and(|span| span.file() == origin.path())
    {
        return Ok(Some(Blocker::MismatchedErrorContract));
    }
    let Some(flow) = ir.function_call_flow(function)? else {
        return Ok(Some(Blocker::UntrustedNamespace));
    };
    if scope.opaque() || !flow.namespace_trusted() || flow.source_digest() != facts.source_digest()
    {
        return Ok(Some(Blocker::UntrustedNamespace));
    }
    Ok(None)
}

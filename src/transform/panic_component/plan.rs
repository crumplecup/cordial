//! Compose snapshot-bound declaration, return, and propagation edits.

use super::super::{
    ErrorDeclarationDecision, FunctionEdit, FunctionEditPlan, ReturnLiftDecision,
    plan_owned_error_declaration, plan_owned_error_return_lift,
};
use super::{PanicComponentBlocker, PanicComponentDecision, PanicComponentPreview, callers};
use crate::error::CordialResult;
use crate::ir::{IrView, NodeId};
use std::ops::ControlFlow;

/// Preview one resolved I/O panic migration through transitive known callers.
///
/// Walk incoming edges to a fixed point, stopping at the explicit boundary.
/// Recursive relationships and repeated sites share one signature lift. All
/// evidence and edit coordinates belong to the same shared source snapshot.
#[tracing::instrument(level = "debug", skip(ir, leaf, boundary), err(level = "warn"))]
#[hotpath::measure]
pub fn plan_owned_panic_component(
    ir: &dyn IrView,
    leaf: NodeId,
    site_index: usize,
    boundary: NodeId,
) -> CordialResult<PanicComponentDecision> {
    use PanicComponentBlocker as Blocker;
    use PanicComponentDecision::{NoCandidate, Ready, Refused};
    let declaration = match plan_owned_error_declaration(ir, leaf, site_index)? {
        ErrorDeclarationDecision::Ready(plan) => plan,
        ErrorDeclarationDecision::NoCandidate => return Ok(NoCandidate),
        ErrorDeclarationDecision::Refused(reason) => {
            return Ok(Refused(Blocker::Declaration(reason)));
        }
    };
    let functions = match callers::collect(ir, leaf, boundary) {
        ControlFlow::Continue(functions) => functions,
        ControlFlow::Break(reason) => return Ok(Refused(reason)),
    };
    let mut success_edits = Vec::new();
    let mut propagation_edits = Vec::new();
    let mut ordered_functions: Vec<_> = functions.iter().copied().collect();
    ordered_functions.sort_by_key(|function| function.value());
    for function in &ordered_functions {
        let lift = match plan_owned_error_return_lift(ir, *function, &declaration)? {
            ReturnLiftDecision::Ready(plan) => plan,
            ReturnLiftDecision::Refused(reason) => {
                return Ok(Refused(Blocker::ReturnContract {
                    function: *function,
                    reason,
                }));
            }
            ReturnLiftDecision::AlreadyLifted => return Ok(Refused(Blocker::InvalidEvidence)),
        };
        let Some(facts) = ir.function_source_syntax(*function)? else {
            return Ok(Refused(Blocker::InvalidEvidence));
        };
        let allowed_sites = usize::from(*function == leaf);
        if facts.unwraps().len() != allowed_sites {
            return Ok(Refused(Blocker::AdditionalPanicSites(*function)));
        }
        match callers::propagation(ir, *function, &facts, &functions)? {
            ControlFlow::Continue(edits) => propagation_edits.extend(edits),
            ControlFlow::Break(reason) => return Ok(Refused(reason)),
        }
        success_edits.extend(lift.edits().iter().cloned());
    }
    let Some(facts) = ir.function_source_syntax(leaf)? else {
        return Ok(Refused(Blocker::InvalidEvidence));
    };
    let Some(failure) = facts
        .unwraps()
        .get(site_index)
        .and_then(|site| super::super::failure_propagation::suffix_edits(&facts, site))
    else {
        return Ok(Refused(Blocker::InvalidEvidence));
    };
    propagation_edits.extend(failure);
    // Propagation insertions precede success-closing insertions at equal offsets.
    propagation_edits.extend(success_edits);
    propagation_edits.extend(declaration.plan().edits().iter().cloned());
    propagation_edits.sort_by_key(|edit: &FunctionEdit| (edit.range().start(), edit.range().end()));
    if propagation_edits
        .windows(2)
        .any(|pair| pair[0].range().end() > pair[1].range().start())
    {
        return Ok(Refused(Blocker::InvalidEvidence));
    }
    let origin = declaration.plan();
    Ok(Ready(PanicComponentPreview::new(
        FunctionEditPlan::new(
            leaf,
            origin.path().clone(),
            origin.source_digest(),
            propagation_edits,
        ),
        ordered_functions,
        boundary,
    )))
}

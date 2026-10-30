//! Compose snapshot-bound declaration, return, and propagation edits.

use super::super::{
    ErrorDeclarationDecision, FunctionEdit, FunctionEditPlan, ReturnLiftDecision,
    plan_owned_error_group_declaration, plan_owned_error_return_lift,
};
use super::{
    PanicComponentBlocker, PanicComponentDecision, PanicComponentPreview, PanicSiteSelection,
    callers,
};
use crate::error::CordialResult;
use crate::ir::{IrView, NodeId};
use std::ops::ControlFlow;

/// Preview one resolved native panic migration through transitive known callers.
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
    plan_owned_panic_group(ir, &[PanicSiteSelection::new(leaf, site_index)], boundary)
}

/// Compose all selected native failures with one declaration and caller closure.
///
/// Selection order does not affect names or edits. Each affected function must
/// have every unwrap selected; duplicates and incompatible sources refuse the
/// entire preview. Shared and recursive callers acquire one return adaptation.
#[tracing::instrument(level = "debug", skip(ir, sites, boundary), err(level = "warn"))]
#[hotpath::measure]
pub fn plan_owned_panic_group(
    ir: &dyn IrView,
    sites: &[PanicSiteSelection],
    boundary: NodeId,
) -> CordialResult<PanicComponentDecision> {
    use PanicComponentBlocker as Blocker;
    use PanicComponentDecision::{NoCandidate, Ready, Refused};
    let mut sites = sites.to_vec();
    sites.sort_by_key(|site| (site.function().value(), site.index()));
    let Some(first) = sites.first() else {
        return Ok(NoCandidate);
    };
    if sites.windows(2).any(|pair| pair[0] == pair[1]) {
        return Ok(Refused(Blocker::InvalidEvidence));
    }
    let leaf = first.function();
    let declaration = match plan_owned_error_group_declaration(ir, &sites)? {
        ErrorDeclarationDecision::Ready(plan) => plan,
        ErrorDeclarationDecision::NoCandidate => return Ok(NoCandidate),
        ErrorDeclarationDecision::Refused(reason) => {
            return Ok(Refused(Blocker::Declaration(reason)));
        }
    };
    let mut functions = std::collections::HashSet::new();
    let mut success_edits = Vec::new();
    let mut propagation_edits = Vec::new();
    for selected in &sites {
        let Some(facts) = ir.function_source_syntax(selected.function())? else {
            return Ok(Refused(Blocker::InvalidEvidence));
        };
        let Some(site) = facts.unwraps().get(selected.index()) else {
            return Ok(Refused(Blocker::InvalidEvidence));
        };
        match callers::collect(ir, selected.function(), boundary) {
            ControlFlow::Continue(callers) => functions.extend(callers),
            ControlFlow::Break(reason) => return Ok(Refused(reason)),
        }
        let Some(edits) = super::super::failure_propagation::suffix_edits(&facts, site) else {
            return Ok(Refused(Blocker::InvalidEvidence));
        };
        propagation_edits.extend(edits);
    }
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
        let allowed_sites = sites
            .iter()
            .filter(|site| site.function() == *function)
            .count();
        if facts.unwraps().len() != allowed_sites {
            return Ok(Refused(Blocker::AdditionalPanicSites(*function)));
        }
        match callers::propagation(ir, *function, &facts, &functions)? {
            ControlFlow::Continue(edits) => propagation_edits.extend(edits),
            ControlFlow::Break(reason) => return Ok(Refused(reason)),
        }
        success_edits.extend(lift.edits().iter().cloned());
    }
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
        declaration.names().clone(),
        sites,
    )))
}

//! Adapt a resolved caller to a separately planned generated error contract.

use std::ops::ControlFlow;

use crate::error::CordialResult;
use crate::ir::{IrView, NodeId, SourceCallResolution};

use super::{
    FailurePropagationBlocker, FunctionEdit, FunctionEditPlan, OwnedErrorDeclaration,
    ReturnLiftBlocker, ReturnLiftDecision, plan_owned_error_return_lift,
};

/// Outcome of adapting one caller, without claiming complete caller closure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallerAdaptationDecision {
    /// Both required signature plans are retained with the adapted caller.
    Ready(OwnedCallerAdaptation),
    /// The selected call does not exist in this source snapshot.
    NoCandidate,
    /// An unresolved obligation prevents any partial output.
    Refused(CallerAdaptationBlocker),
}

/// Evidence that prevents this caller operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallerAdaptationBlocker {
    /// Source resolution or snapshot identities disagree.
    CallEvidence(FailurePropagationBlocker),
    /// One signature cannot use the selected generated error contract.
    ReturnContract(ReturnLiftBlocker),
    /// Recursive signatures need a component operation.
    RecursiveCall,
    /// A call's range is outside its owning body.
    InvalidRange,
}

/// An isolated caller adaptation and the callee lift on which it depends.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters, derive_new::new)]
#[new(visibility = "pub(super)")]
pub struct OwnedCallerAdaptation {
    caller: FunctionEditPlan,
    callee: FunctionEditPlan,
}

/// Lift one caller and propagate every resolved call to the selected callee.
///
/// The callee's return lift is planned against the same declaration and retained
/// as a dependency. This operation neither changes the callee's failures nor
/// proves that all other callers and function-value uses are covered.
#[tracing::instrument(level = "debug", skip(ir, caller, declaration), err(level = "warn"))]
#[hotpath::measure]
pub fn plan_owned_caller_adaptation(
    ir: &dyn IrView,
    caller: NodeId,
    call_index: usize,
    declaration: &OwnedErrorDeclaration,
) -> CordialResult<CallerAdaptationDecision> {
    use CallerAdaptationBlocker as Blocker;
    use CallerAdaptationDecision::{NoCandidate, Ready, Refused};
    let Some(facts) = ir.function_source_syntax(caller)? else {
        return Ok(Refused(Blocker::CallEvidence(
            FailurePropagationBlocker::MissingFunctionEvidence,
        )));
    };
    let Some(call) = facts.calls().get(call_index) else {
        return Ok(NoCandidate);
    };
    let target = match super::call_target::resolved_call(ir, caller, &facts, call.range())? {
        ControlFlow::Break(blocker) => return Ok(Refused(Blocker::CallEvidence(blocker))),
        ControlFlow::Continue((target, _)) => target,
    };
    if caller == target {
        return Ok(Refused(Blocker::RecursiveCall));
    }
    let callee = match plan_owned_error_return_lift(ir, target, declaration)? {
        ReturnLiftDecision::Ready(plan) => plan,
        ReturnLiftDecision::Refused(blocker) => {
            return Ok(Refused(Blocker::ReturnContract(blocker)));
        }
        ReturnLiftDecision::AlreadyLifted => {
            return Ok(Refused(Blocker::ReturnContract(
                ReturnLiftBlocker::AlreadyFallible,
            )));
        }
    };
    let caller_plan = match plan_owned_error_return_lift(ir, caller, declaration)? {
        ReturnLiftDecision::Ready(plan) => plan,
        ReturnLiftDecision::Refused(blocker) => {
            return Ok(Refused(Blocker::ReturnContract(blocker)));
        }
        ReturnLiftDecision::AlreadyLifted => {
            return Ok(Refused(Blocker::ReturnContract(
                ReturnLiftBlocker::AlreadyFallible,
            )));
        }
    };
    let mut edits = Vec::new();
    let Some(flow) = ir.function_call_flow(caller)? else {
        return Ok(Refused(Blocker::CallEvidence(
            FailurePropagationBlocker::MissingCallEvidence,
        )));
    };
    for call in flow.calls() {
        if matches!(call.resolution(), SourceCallResolution::Source { function, .. } if *function == target)
        {
            if let ControlFlow::Break(blocker) =
                super::call_target::resolved_call(ir, caller, &facts, call.range())?
            {
                return Ok(Refused(Blocker::CallEvidence(blocker)));
            }
            let range = call.range();
            if facts.body_range().start() >= range.start()
                || range.start() >= range.end()
                || range.end() >= facts.body_range().end()
            {
                return Ok(Refused(Blocker::InvalidRange));
            }
            edits.push(FunctionEdit::new(
                (range.end()..range.end()).into(),
                "?".to_owned(),
            ));
        }
    }
    // Stable ordering puts propagation before a closing Ok at the same offset:
    // Ok(callee()?), including tail and explicit-return expressions.
    edits.extend(caller_plan.edits().iter().cloned());
    edits.sort_by_key(|edit| (edit.range().start(), edit.range().end()));
    Ok(Ready(OwnedCallerAdaptation::new(
        FunctionEditPlan::new(
            caller,
            caller_plan.path().clone(),
            facts.source_digest(),
            edits,
        ),
        callee,
    )))
}

//! Establish family dependencies without interpreting source text again.

use std::collections::{BTreeMap, BTreeSet};

use super::{FamilyState, FunctionSourceSyntax, IrView, NodeId};
use crate::error::CordialResult;
use crate::ir::{
    EdgeKind, ErrorFamilyBasis, ErrorFamilyGapReason as Gap, FunctionBoundary,
    SourceCallResolution, SourceCallUse,
};

#[tracing::instrument(
    level = "trace",
    skip(ir, function, facts, functions),
    err(level = "warn")
)]
pub(super) fn collect(
    ir: &dyn IrView,
    function: NodeId,
    facts: &FunctionSourceSyntax,
    functions: &BTreeMap<u32, FunctionSourceSyntax>,
) -> CordialResult<FamilyState> {
    let mut state = FamilyState {
        digest: facts.source_digest(),
        basis: if facts.result_contract().is_some() {
            ErrorFamilyBasis::DeclaredResult
        } else {
            ErrorFamilyBasis::PanicMigration
        },
        members: BTreeMap::new(),
        gaps: BTreeSet::new(),
        dependencies: BTreeSet::new(),
    };
    let Some(flow) = ir.function_call_flow(function)? else {
        state.gap(function, None, Gap::MissingOrInvalidEvidence);
        return Ok(state);
    };
    if !flow.namespace_trusted()
        || flow.source_digest() != facts.source_digest()
        || flow.calls().len() != facts.calls().len()
        || !flow
            .calls()
            .iter()
            .zip(facts.calls())
            .all(|(resolved, call)| resolved.range() == call.range())
    {
        state.gap(function, None, Gap::MissingOrInvalidEvidence);
        return Ok(state);
    }
    // A declared contract is a terminal boundary, not an assertion about the
    // implementation's reachable error values or hidden panic behavior.
    if let Some(contract) = facts.result_contract() {
        if let Some(identity) = contract.canonical_error_identity() {
            if identity != "::core::convert::Infallible" {
                state
                    .members
                    .entry(identity.to_owned())
                    .or_default()
                    .insert(function.value());
            }
        } else {
            state.gap(function, None, Gap::UnresolvedErrorType);
        }
        return Ok(state);
    }
    if facts.return_flow().boundaries().iter().any(|boundary| {
        matches!(
            boundary,
            FunctionBoundary::SpecialSignature
                | FunctionBoundary::Generic
                | FunctionBoundary::Attribute
                | FunctionBoundary::Macro
                | FunctionBoundary::NestedScope
                | FunctionBoundary::NeverOutput
        )
    }) {
        state.gap(function, None, Gap::UnsupportedOperation);
    }
    if let Some(opaque) = facts.opaque_calls() {
        for range in opaque {
            state.gap(function, Some(range), Gap::UnsupportedOperation);
        }
    } else {
        state.gap(function, None, Gap::MissingOrInvalidEvidence);
    }
    for (call, resolved) in facts.calls().iter().zip(flow.calls()) {
        let SourceCallResolution::Source {
            function: target,
            source_digest,
        } = resolved.resolution()
        else {
            state.gap(function, Some(call.range()), Gap::UnresolvedCall);
            continue;
        };
        let Some(callee) = functions.get(&target.value()).filter(|callee| {
            callee.source_digest() == *source_digest
                && ir.children(function, EdgeKind::Calls).contains(target)
        }) else {
            state.gap(function, Some(call.range()), Gap::MissingOrInvalidEvidence);
            continue;
        };
        match (call.usage(), callee.result_contract().is_some()) {
            (Some(SourceCallUse::Value), false) | (Some(SourceCallUse::Unwrap), true) => {
                state.dependencies.insert(target.value());
            }
            (Some(SourceCallUse::HandledResult), true) => {}
            (Some(SourceCallUse::Value | SourceCallUse::Residual), true) => {
                state.gap(function, Some(call.range()), Gap::UnsupportedResultUse);
            }
            (None, _) => state.gap(function, Some(call.range()), Gap::MissingOrInvalidEvidence),
            _ => state.gap(function, Some(call.range()), Gap::UnprovenResultReceiver),
        }
    }
    Ok(state)
}

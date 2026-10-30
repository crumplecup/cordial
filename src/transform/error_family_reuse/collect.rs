//! Canonicalize complete families while retaining their producer identities.

use std::collections::{BTreeMap, BTreeSet};

use super::{
    ErrorFamilyBasis, ErrorFamilyDisposition, ErrorFamilyMember,
    ErrorFamilyReuseBlocker as Blocker, ErrorFamilyReuseIssue, FamilyKey, IrView, NodeId,
    ReusableErrorFamily,
};
use crate::error::CordialResult;
use crate::ir::FunctionErrorFamily;

#[tracing::instrument(level = "trace", skip(ir, functions, basis), err(level = "warn"))]
pub(super) fn collect(
    ir: &dyn IrView,
    functions: &[NodeId],
    basis: ErrorFamilyBasis,
) -> CordialResult<(Vec<ReusableErrorFamily>, Vec<ErrorFamilyReuseIssue>)> {
    let mut groups: BTreeMap<FamilyKey, Vec<NodeId>> = BTreeMap::new();
    let mut issues = Vec::new();
    let functions: BTreeSet<_> = functions.iter().map(|id| id.value()).collect();
    for function in functions.into_iter().map(NodeId::new) {
        match checked_family(ir, function, basis)? {
            Ok(key) => groups.entry(key).or_default().push(function),
            Err(reason) => issues.push(ErrorFamilyReuseIssue::new(function, reason)),
        }
    }
    let groups = groups
        .into_iter()
        .map(|(key, functions)| ReusableErrorFamily {
            members: key
                .into_iter()
                .map(|(identity, origins)| {
                    ErrorFamilyMember::new(identity, origins.into_iter().map(NodeId::new).collect())
                })
                .collect(),
            functions,
            disposition: ErrorFamilyDisposition::BelowThreshold,
        })
        .collect();
    Ok((groups, issues))
}

#[tracing::instrument(level = "trace", skip(ir, function, basis), err(level = "warn"))]
fn checked_family(
    ir: &dyn IrView,
    function: NodeId,
    basis: ErrorFamilyBasis,
) -> CordialResult<Result<FamilyKey, Blocker>> {
    let Some(facts) = ir.function_source_syntax(function)? else {
        return Ok(Err(Blocker::MissingFunction));
    };
    let Some(family) = ir.function_error_family(function)? else {
        return Ok(Err(Blocker::MissingFamily));
    };
    let trusted = ir.function_call_flow(function)?.is_some_and(|flow| {
        flow.namespace_trusted() && flow.source_digest() == facts.source_digest()
    });
    if !trusted
        || family.source_digest() != facts.source_digest()
        || (family.basis() == ErrorFamilyBasis::DeclaredResult) != facts.result_contract().is_some()
    {
        return Ok(Err(Blocker::InvalidEvidence));
    }
    if family.basis() != basis {
        return Ok(Err(Blocker::DifferentBasis));
    }
    if !family.gaps().is_empty() {
        return Ok(Err(Blocker::Incomplete));
    }
    canonical_members(ir, &family)
}

#[tracing::instrument(level = "trace", skip(ir, family), err(level = "warn"))]
fn canonical_members(
    ir: &dyn IrView,
    family: &FunctionErrorFamily,
) -> CordialResult<Result<FamilyKey, Blocker>> {
    let mut members = BTreeMap::new();
    for member in family.members() {
        let origins: BTreeSet<_> = member.origins().iter().map(|id| id.value()).collect();
        if origins.is_empty()
            || origins.len() != member.origins().len()
            || members.contains_key(member.identity())
            || member.identity() == "::core::convert::Infallible"
        {
            return Ok(Err(Blocker::InvalidEvidence));
        }
        for origin in &origins {
            let origin = NodeId::new(*origin);
            let Some(facts) = ir.function_source_syntax(origin)? else {
                return Ok(Err(Blocker::InvalidEvidence));
            };
            let trusted = ir.function_call_flow(origin)?.is_some_and(|flow| {
                flow.namespace_trusted() && flow.source_digest() == facts.source_digest()
            });
            let identity = facts
                .result_contract()
                .as_ref()
                .and_then(|contract| contract.canonical_error_identity());
            if !trusted || identity != Some(member.identity().as_str()) {
                return Ok(Err(Blocker::InvalidEvidence));
            }
        }
        members.insert(member.identity().clone(), origins.into_iter().collect());
    }
    Ok(Ok(members.into_iter().collect()))
}

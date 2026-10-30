//! Separate exact shared evidence from proposed contracts under nesting policy.

use crate::error::CordialResult;
use crate::ir::{ErrorFamilyMember, IrView, NodeId};

use super::{
    ErrorFamilyConflict, ErrorFamilyReusePlan, ErrorFamilyReusePolicy, SourceErrorReuse,
    plan_error_family_reuse,
};

mod closure;

/// Whether representation may add alternatives absent from an exact family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorContractBroadening {
    /// Report the proposed additions as blockers instead of accepting them.
    Forbidden,
    /// Accept explicit additions required by the no-source-reuse policy.
    Permitted,
}

/// Proposed representation for one exact group in the enclosed reuse preview.
///
/// These are analysis results, not declarations or rewrite authority. Members
/// in the reuse preview remain the exact evidence; additions never change IR.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters)]
pub struct ErrorFamilyContract {
    #[getter(copy)]
    exact_group: usize,
    proposed_members: Vec<ErrorFamilyMember>,
    added_members: Vec<ErrorFamilyMember>,
    /// Groups whose crossing contracts directly caused an expansion.
    broadened_by: Vec<usize>,
}

/// An obligation that prevents accepting the complete representation preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorFamilyRepresentationBlocker {
    /// At least one requested function failed shared evidence validation.
    IncompleteEvidence,
    /// Producer differences cannot be erased, even when broadening is allowed.
    IncompatibleFamilies {
        /// Index into this preview's exact groups and contracts.
        left_group: usize,
        /// Index into this preview's exact groups and contracts.
        right_group: usize,
        /// Nominal overlap does not authorize merging domain provenance.
        conflict: ErrorFamilyConflict,
    },
    /// Proposed alternatives exceed an exact contract and policy forbids that.
    BroadeningRequired {
        /// Index into this preview's exact groups and contracts.
        group: usize,
    },
}

/// A deterministic feasibility preview, not an optimality or rewrite proof.
///
/// Consumers must check blockers before accepting any representation. When
/// source reuse is forbidden and blockers are empty, all proposed member sets
/// are disjoint or nested, including those below the reuse threshold. This
/// does not choose declaration placement, names, storage, or conversions.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters)]
pub struct ErrorFamilyRepresentationPlan {
    reuse: ErrorFamilyReusePlan,
    #[getter(copy)]
    reuse_policy: ErrorFamilyReusePolicy,
    #[getter(copy)]
    broadening_policy: ErrorContractBroadening,
    contracts: Vec<ErrorFamilyContract>,
    blockers: Vec<ErrorFamilyRepresentationBlocker>,
}

/// Keep exact contracts when source reuse is allowed. Otherwise preserve
/// selected frequent families and expand crossing unselected contracts to a
/// fixed point. Report every addition or refuse it according to explicit
/// policy. Distinct producer sets are never silently merged.
#[tracing::instrument(
    level = "debug",
    skip(ir, functions, reuse_policy, broadening_policy),
    err(level = "warn")
)]
#[hotpath::measure]
pub fn plan_error_family_representation(
    ir: &dyn IrView,
    functions: &[NodeId],
    reuse_policy: ErrorFamilyReusePolicy,
    broadening_policy: ErrorContractBroadening,
) -> CordialResult<ErrorFamilyRepresentationPlan> {
    let reuse = plan_error_family_reuse(ir, functions, reuse_policy)?;
    let mut blockers = Vec::new();
    if !reuse.issues().is_empty() {
        blockers.push(ErrorFamilyRepresentationBlocker::IncompleteEvidence);
    }
    let mut contracts: Vec<_> = reuse
        .groups()
        .iter()
        .enumerate()
        .map(|(exact_group, group)| ErrorFamilyContract {
            exact_group,
            proposed_members: group.members().clone(),
            added_members: Vec::new(),
            broadened_by: Vec::new(),
        })
        .collect();
    if reuse_policy.source_reuse() == SourceErrorReuse::Forbidden {
        blockers.extend(closure::close(&reuse, &mut contracts));
    }
    for contract in &mut contracts {
        let exact = reuse.groups()[contract.exact_group].members();
        contract.added_members = contract
            .proposed_members
            .iter()
            .filter(|member| !exact.contains(member))
            .cloned()
            .collect();
        contract.broadened_by.sort_unstable();
        if !contract.added_members.is_empty()
            && broadening_policy == ErrorContractBroadening::Forbidden
        {
            blockers.push(ErrorFamilyRepresentationBlocker::BroadeningRequired {
                group: contract.exact_group,
            });
        }
    }
    Ok(ErrorFamilyRepresentationPlan {
        reuse,
        reuse_policy,
        broadening_policy,
        contracts,
        blockers,
    })
}

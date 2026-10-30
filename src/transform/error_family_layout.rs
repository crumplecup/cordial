//! Assign represented contracts to distinct families and nonduplicating children.

use crate::error::CordialResult;
use crate::ir::{ErrorFamilyMember, IrView, NodeId};

use super::{
    ErrorContractBroadening, ErrorFamilyRepresentationPlan, ErrorFamilyReusePolicy,
    plan_error_family_representation,
};

mod collect;
mod nesting;

/// A declaration-layout preview or the representation obligations blocking it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorFamilyLayoutDecision {
    /// Every requested contract has a represented family or is infallible.
    Ready(ErrorFamilyLayoutPlan),
    /// No partial layout is exposed when representation has blockers.
    Refused(ErrorFamilyRepresentationPlan),
}

/// One distinct represented set, before names, placement, storage or rendering.
///
/// A family contains its direct alternatives and its child families. Each
/// alternative occurs exactly once within that expansion. Children always
/// have strictly fewer members, so declarations cannot form recursive cycles.
/// Child edges describe error families carried through native source wrappers
/// containing their own Kind, rather than direct Kind-in-Kind payloads.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters)]
pub struct ErrorFamilyLayoutNode {
    members: Vec<ErrorFamilyMember>,
    /// Exact groups sharing this represented set, including broadened groups.
    exact_groups: Vec<usize>,
    #[getter(copy)]
    function_count: usize,
    /// Whether an exact selected family justifies preferred nesting under reuse.
    #[getter(copy)]
    preferred_reuse: bool,
    direct_members: Vec<ErrorFamilyMember>,
    /// Indices into this same plan's families; always smaller than this node.
    children: Vec<usize>,
}

/// Deterministic declaration structure, not source edits or conversion proof.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters)]
pub struct ErrorFamilyLayoutPlan {
    representation: ErrorFamilyRepresentationPlan,
    /// Ordered by increasing set size, then canonical identity/provenance.
    families: Vec<ErrorFamilyLayoutNode>,
    /// One entry per exact group; None denotes an empty error contract.
    contract_families: Vec<Option<usize>>,
}

/// Deduplicate represented contracts and choose nonoverlapping strict subsets.
///
/// With reuse forbidden, every required return contract participates in the
/// nesting tree, even below the preferred reuse threshold, so each source has
/// one direct owner. With reuse permitted, only selected frequent families
/// become children; conflicting overlapping candidates remain distinct return
/// contracts. Neither policy authorizes editing or blanket From conversions.
#[tracing::instrument(
    level = "debug",
    skip(ir, functions, reuse_policy, broadening_policy),
    err(level = "warn")
)]
#[hotpath::measure]
pub fn plan_error_family_layout(
    ir: &dyn IrView,
    functions: &[NodeId],
    reuse_policy: ErrorFamilyReusePolicy,
    broadening_policy: ErrorContractBroadening,
) -> CordialResult<ErrorFamilyLayoutDecision> {
    let representation =
        plan_error_family_representation(ir, functions, reuse_policy, broadening_policy)?;
    if !representation.blockers().is_empty() {
        return Ok(ErrorFamilyLayoutDecision::Refused(representation));
    }
    let (mut families, contract_families) = collect::collect(&representation);
    nesting::connect(&mut families, reuse_policy.source_reuse());
    Ok(ErrorFamilyLayoutDecision::Ready(ErrorFamilyLayoutPlan {
        representation,
        families,
        contract_families,
    }))
}

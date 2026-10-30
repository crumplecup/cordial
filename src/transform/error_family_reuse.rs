//! Select reusable contracts from shared family evidence without generating code.

use std::num::NonZeroUsize;

use crate::error::CordialResult;
use crate::ir::{ErrorFamilyBasis, ErrorFamilyMember, IrView, NodeId};

mod collect;
mod select;

/// Whether distinct generated families may directly contain the same source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceErrorReuse {
    /// Preserve overlapping exact contracts as separate candidate families.
    Permitted,
    /// Select only disjoint or consistently nested families.
    Forbidden,
}

/// Explicit analysis policy; no configuration default or rewrite authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, derive_getters::Getters, derive_new::new)]
pub struct ErrorFamilyReusePolicy {
    #[getter(copy)]
    basis: ErrorFamilyBasis,
    #[getter(copy)]
    minimum_functions: NonZeroUsize,
    #[getter(copy)]
    source_reuse: SourceErrorReuse,
}

/// Why two exact families cannot share a tree under the no-reuse policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorFamilyConflict {
    /// Neither set contains the other, despite sharing a source identity.
    CrossingOverlap,
    /// An overlapping nominal type has different producing declarations.
    DistinctProducers,
}

/// Selection is deterministic, but is not a globally optimal packing claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorFamilyDisposition {
    /// Meets the threshold and is compatible with earlier selected families.
    Selected,
    /// An empty contract needs no error declaration.
    Infallible,
    /// Fewer distinct functions share this exact contract than policy requires.
    BelowThreshold,
    /// The index refers to a preceding selected group in this same preview.
    Overlap {
        /// Earlier selected group responsible for the conflict.
        selected_group: usize,
        /// Whether shape or producer identity prevents consistent nesting.
        conflict: ErrorFamilyConflict,
    },
}

/// One exact set, including producer identities, and its distinct consumers.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters)]
pub struct ReusableErrorFamily {
    members: Vec<ErrorFamilyMember>,
    functions: Vec<NodeId>,
    #[getter(copy)]
    disposition: ErrorFamilyDisposition,
}

/// A requested function excluded before exact-set grouping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorFamilyReuseBlocker {
    /// No shared source function facts exist for this node.
    MissingFunction,
    /// No enriched error-family summary exists.
    MissingFamily,
    /// Family, source, or call evidence refers to inconsistent snapshots.
    InvalidEvidence,
    /// Signature and prospective migration summaries cannot be combined.
    DifferentBasis,
    /// The function's located gap inventory must be resolved first.
    Incomplete,
}

/// Unresolved functions remain visible instead of inflating a candidate count.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters, derive_new::new)]
#[new(visibility = "pub(super)")]
pub struct ErrorFamilyReuseIssue {
    #[getter(copy)]
    function: NodeId,
    #[getter(copy)]
    reason: ErrorFamilyReuseBlocker,
}

/// Analysis preview only. A rejected group retains its exact original members.
///
/// Under `Forbidden`, an overlapping rejected contract still needs an explicit
/// representation decision: broadening or refusal. This preview neither
/// broadens the contract nor authorizes a declaration or conversion.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters)]
pub struct ErrorFamilyReusePlan {
    groups: Vec<ReusableErrorFamily>,
    issues: Vec<ErrorFamilyReuseIssue>,
}

/// Count distinct functions in the supplied migration scope, then select
/// frequent exact families. Repeated calls and duplicate input ids do not count
/// again. Producer identity participates in equality; foreign type equality
/// alone does not make two domain operations interchangeable.
#[tracing::instrument(level = "debug", skip(ir, functions, policy), err(level = "warn"))]
#[hotpath::measure]
pub fn plan_error_family_reuse(
    ir: &dyn IrView,
    functions: &[NodeId],
    policy: ErrorFamilyReusePolicy,
) -> CordialResult<ErrorFamilyReusePlan> {
    let (mut groups, issues) = collect::collect(ir, functions, policy.basis)?;
    select::select(&mut groups, policy);
    Ok(ErrorFamilyReusePlan { groups, issues })
}

type FamilyKey = Vec<(String, Vec<u32>)>;

/// Canonical member sets must agree on producer identity before nesting.
#[tracing::instrument(level = "trace", skip(left, right))]
pub(super) fn conflict(
    left: &[ErrorFamilyMember],
    right: &[ErrorFamilyMember],
) -> Option<ErrorFamilyConflict> {
    let mut shared = 0;
    for member in left {
        if let Some(other) = right
            .iter()
            .find(|other| other.identity() == member.identity())
        {
            if member.origins() != other.origins() {
                return Some(ErrorFamilyConflict::DistinctProducers);
            }
            shared += 1;
        }
    }
    (shared != 0 && shared != left.len() && shared != right.len())
        .then_some(ErrorFamilyConflict::CrossingOverlap)
}

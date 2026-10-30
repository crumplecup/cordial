//! Deterministic frequent-first selection, with explicit overlap reasons.

use super::{
    ErrorFamilyConflict, ErrorFamilyDisposition as Disposition, ErrorFamilyMember,
    ErrorFamilyReusePolicy, ReusableErrorFamily, SourceErrorReuse,
};

#[tracing::instrument(level = "trace", skip(groups, policy))]
pub(super) fn select(groups: &mut [ReusableErrorFamily], policy: ErrorFamilyReusePolicy) {
    // Stable sort retains the collector's canonical identity order on ties.
    // Frequent first; prefer larger contracts when counts match.
    // No claim of solving the global set-packing optimization problem.
    groups.sort_by(|left, right| {
        right
            .functions
            .len()
            .cmp(&left.functions.len())
            .then_with(|| right.members.len().cmp(&left.members.len()))
    });
    for index in 0..groups.len() {
        let disposition = if groups[index].members.is_empty() {
            Disposition::Infallible
        } else if groups[index].functions.len() < policy.minimum_functions.get() {
            Disposition::BelowThreshold
        } else {
            choose(&groups[..index], &groups[index], policy.source_reuse)
        };
        groups[index].disposition = disposition;
    }
}

#[tracing::instrument(level = "trace", skip(selected, candidate, policy))]
fn choose(
    selected: &[ReusableErrorFamily],
    candidate: &ReusableErrorFamily,
    policy: SourceErrorReuse,
) -> Disposition {
    if policy == SourceErrorReuse::Forbidden {
        for (selected_group, group) in selected.iter().enumerate() {
            if group.disposition == Disposition::Selected {
                if let Some(conflict) = conflict(&group.members, &candidate.members) {
                    return Disposition::Overlap {
                        selected_group,
                        conflict,
                    };
                }
            }
        }
    }
    Disposition::Selected
}

#[tracing::instrument(level = "trace", skip(left, right))]
fn conflict(
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

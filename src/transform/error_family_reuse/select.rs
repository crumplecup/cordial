//! Deterministic frequent-first selection, with explicit overlap reasons.

use super::{
    ErrorFamilyDisposition as Disposition, ErrorFamilyReusePolicy, ReusableErrorFamily,
    SourceErrorReuse, conflict,
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

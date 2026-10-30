//! Choose disjoint children without hiding or duplicating any alternative.

use super::ErrorFamilyLayoutNode;
use crate::transform::SourceErrorReuse;

#[tracing::instrument(level = "trace", skip(families, policy))]
pub(super) fn connect(families: &mut [ErrorFamilyLayoutNode], policy: SourceErrorReuse) {
    for parent in 0..families.len() {
        let children = children(families, parent, policy);
        let direct_members = families[parent]
            .members
            .iter()
            .filter(|member| {
                !children
                    .iter()
                    .any(|child| families[*child].members.contains(member))
            })
            .cloned()
            .collect();
        families[parent].children = children;
        families[parent].direct_members = direct_members;
    }
}

#[tracing::instrument(level = "trace", skip(families, policy))]
fn children(
    families: &[ErrorFamilyLayoutNode],
    parent: usize,
    policy: SourceErrorReuse,
) -> Vec<usize> {
    let members = &families[parent].members;
    let mut candidates: Vec<_> = families[..parent]
        .iter()
        .enumerate()
        .filter(|(_, family)| {
            family.members.len() < members.len()
                && family.members.iter().all(|member| members.contains(member))
                && (policy == SourceErrorReuse::Forbidden || family.preferred_reuse)
        })
        .map(|(index, _)| index)
        .collect();
    candidates.sort_by(|left, right| {
        let a = &families[*left];
        let b = &families[*right];
        let frequency = if policy == SourceErrorReuse::Permitted {
            b.function_count.cmp(&a.function_count)
        } else {
            // Maximal proper subsets preserve one owner per source under the
            // no-reuse policy, irrespective of preferred reuse frequency.
            std::cmp::Ordering::Equal
        };
        frequency
            .then_with(|| b.members.len().cmp(&a.members.len()))
            .then_with(|| left.cmp(right))
    });
    let mut children: Vec<usize> = Vec::new();
    for candidate in candidates {
        let overlaps = children.iter().any(|chosen| {
            families[candidate]
                .members
                .iter()
                .any(|member| families[*chosen].members.contains(member))
        });
        if !overlaps {
            children.push(candidate);
        }
    }
    children.sort_unstable();
    children
}

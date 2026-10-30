//! Close every represented contract, including infrequent families, under policy.

use super::{ErrorFamilyContract, ErrorFamilyRepresentationBlocker as Blocker};
use crate::transform::error_family_reuse::conflict;
use crate::transform::{ErrorFamilyConflict, ErrorFamilyDisposition, ErrorFamilyReusePlan};

#[tracing::instrument(level = "trace", skip(reuse, contracts))]
pub(super) fn close(
    reuse: &ErrorFamilyReusePlan,
    contracts: &mut [ErrorFamilyContract],
) -> Vec<Blocker> {
    // Reject conflicting provenance before computing unions. No-source-reuse
    // applies to the whole scope, not only the frequent selected groups.
    let mut blockers = Vec::new();
    for (left, contract) in contracts.iter().enumerate() {
        for (right, other) in contracts.iter().enumerate().skip(left + 1) {
            if conflict(&contract.proposed_members, &other.proposed_members)
                == Some(ErrorFamilyConflict::DistinctProducers)
            {
                blockers.push(Blocker::IncompatibleFamilies {
                    left_group: left,
                    right_group: right,
                    conflict: ErrorFamilyConflict::DistinctProducers,
                });
            }
        }
    }
    if !blockers.is_empty() {
        return blockers;
    }
    // Every step adds at least one existing member to an unselected contract.
    // The finite source universe bounds growth. Selected sets stay exact.
    while let Some((left, right)) = crossing(contracts) {
        let frozen_left = reuse.groups()[left].disposition() == ErrorFamilyDisposition::Selected;
        let frozen_right = reuse.groups()[right].disposition() == ErrorFamilyDisposition::Selected;
        if frozen_left && frozen_right {
            blockers.push(Blocker::IncompatibleFamilies {
                left_group: left,
                right_group: right,
                conflict: ErrorFamilyConflict::CrossingOverlap,
            });
            break;
        }
        let mut union = contracts[left].proposed_members.clone();
        union.extend(contracts[right].proposed_members.iter().cloned());
        union.sort_by(|a, b| a.identity().cmp(b.identity()));
        // Full member equality retains producer identity; never dedup by type.
        union.dedup();
        if !frozen_left {
            expand(&mut contracts[left], &union, right);
        }
        if !frozen_right {
            expand(&mut contracts[right], &union, left);
        }
    }
    blockers
}

#[tracing::instrument(level = "trace", skip(contracts))]
fn crossing(contracts: &[ErrorFamilyContract]) -> Option<(usize, usize)> {
    for (left, contract) in contracts.iter().enumerate() {
        for (right, other) in contracts.iter().enumerate().skip(left + 1) {
            if conflict(&contract.proposed_members, &other.proposed_members)
                == Some(ErrorFamilyConflict::CrossingOverlap)
            {
                return Some((left, right));
            }
        }
    }
    None
}

#[tracing::instrument(level = "trace", skip(contract, members))]
fn expand(
    contract: &mut ErrorFamilyContract,
    members: &[crate::ir::ErrorFamilyMember],
    cause: usize,
) {
    contract.proposed_members = members.to_vec();
    if !contract.broadened_by.contains(&cause) {
        contract.broadened_by.push(cause);
    }
}

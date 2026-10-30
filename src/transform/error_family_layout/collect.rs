//! Equal proposed sets share one layout node without losing exact consumers.

use std::collections::BTreeMap;

use super::{ErrorFamilyLayoutNode, ErrorFamilyRepresentationPlan};
use crate::transform::ErrorFamilyDisposition;

#[tracing::instrument(level = "trace", skip(representation))]
pub(super) fn collect(
    representation: &ErrorFamilyRepresentationPlan,
) -> (Vec<ErrorFamilyLayoutNode>, Vec<Option<usize>>) {
    let mut families = BTreeMap::new();
    for contract in representation.contracts() {
        if contract.proposed_members().is_empty() {
            continue;
        }
        let key: Vec<_> = contract
            .proposed_members()
            .iter()
            .map(|member| {
                (
                    member.identity().clone(),
                    member
                        .origins()
                        .iter()
                        .map(|id| id.value())
                        .collect::<Vec<_>>(),
                )
            })
            .collect();
        let family = families
            .entry(key)
            .or_insert_with(|| ErrorFamilyLayoutNode {
                members: contract.proposed_members().clone(),
                exact_groups: Vec::new(),
                function_count: 0,
                preferred_reuse: false,
                direct_members: Vec::new(),
                children: Vec::new(),
            });
        let exact = &representation.reuse().groups()[contract.exact_group()];
        family.exact_groups.push(contract.exact_group());
        family.function_count += exact.functions().len();
        family.preferred_reuse |= exact.disposition() == ErrorFamilyDisposition::Selected;
    }
    let mut families: Vec<_> = families.into_values().collect();
    // Stable sorting preserves canonical identities for equally sized sets.
    families.sort_by_key(|family| family.members.len());
    let mut contract_families = vec![None; representation.contracts().len()];
    for (index, family) in families.iter().enumerate() {
        for group in &family.exact_groups {
            contract_families[*group] = Some(index);
        }
    }
    (families, contract_families)
}

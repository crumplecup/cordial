//! Stable ordering of quality etiquettes by the lint-order table.

use std::collections::BTreeSet;

use tracing::instrument;

use crate::etiquette::order::LintOrder;
use crate::etiquette::order_table::ERROR_HANDLING_RULE_IDS;
use crate::etiquette::quality::QualityEtiquette;

/// Stable topological sort of quality etiquettes: `After` edges become area edges.
#[instrument(level = "debug", skip(etiquettes, order))]
pub(crate) fn sort_quality_etiquettes<'a>(
    etiquettes: &[&'a dyn QualityEtiquette],
    order: &LintOrder,
) -> Vec<&'a dyn QualityEtiquette> {
    let n = etiquettes.len();
    // Kept alive for the whole function: `rule_sets` borrows rule ids from
    // these, and `EtiquetteRuleExplain::id` no longer returns `&'static
    // str` now that the type owns its text.
    let explains: Vec<_> = etiquettes
        .iter()
        .map(|etiquette| etiquette.explain())
        .collect();
    let rule_sets: Vec<Vec<&str>> = explains
        .iter()
        .map(|explain| {
            explain
                .rules()
                .iter()
                .map(|rule| rule.id().as_str())
                .collect()
        })
        .collect();

    let mut indegree = vec![0usize; n];
    let mut edges: Vec<(usize, usize)> = Vec::new();
    for earlier in 0..n {
        for later in 0..n {
            if earlier == later {
                continue;
            }
            if area_precedes(order, &rule_sets[earlier], &rule_sets[later]) {
                edges.push((earlier, later));
                indegree[later] += 1;
            }
        }
    }

    let mut placed = vec![false; n];
    let mut out = Vec::with_capacity(n);
    while out.len() < n {
        let mut next = None;
        for index in 0..n {
            if !placed[index] && indegree[index] == 0 {
                next = Some(index);
                break;
            }
        }
        let Some(node) = next else {
            out.extend(
                (0..n)
                    .filter(|index| !placed[*index])
                    .map(|index| etiquettes[index]),
            );
            break;
        };
        placed[node] = true;
        out.push(etiquettes[node]);
        for (from, to) in &edges {
            if *from == node {
                indegree[*to] -= 1;
            }
        }
    }
    lift_error_handling_cluster(out, order)
}

/// Areas reachable from Error handling sit immediately after that
/// hardcoded first row: foreign error types through proof patterns.
#[instrument(level = "debug", skip(ordered, order))]
fn lift_error_handling_cluster<'a>(
    ordered: Vec<&'a dyn QualityEtiquette>,
    order: &LintOrder,
) -> Vec<&'a dyn QualityEtiquette> {
    let lead = reachable_successors(order, ERROR_HANDLING_RULE_IDS);
    if lead.is_empty() {
        return ordered;
    }
    let (cluster, rest): (Vec<_>, Vec<_>) = ordered.into_iter().partition(|etiquette| {
        etiquette
            .explain()
            .rules()
            .iter()
            .any(|rule| lead.contains(rule.id().as_str()))
    });
    cluster.into_iter().chain(rest).collect()
}

#[instrument(level = "debug", skip(order, seeds))]
fn reachable_successors(order: &LintOrder, seeds: &[&str]) -> BTreeSet<&'static str> {
    let mut seen = BTreeSet::new();
    let mut stack: Vec<&str> = seeds.iter().flat_map(|id| order.successors(id)).collect();
    while let Some(id) = stack.pop() {
        if seen.insert(id) {
            stack.extend(order.successors(id));
        }
    }
    seen
}

#[instrument(level = "debug", skip(order, earlier, later))]
fn area_precedes(order: &LintOrder, earlier: &[&str], later: &[&str]) -> bool {
    earlier.iter().any(|id| {
        order
            .successors(id)
            .iter()
            .any(|successor| later.contains(successor))
    })
}

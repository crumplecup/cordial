use std::collections::{HashMap, HashSet};

use tracing::instrument;

use super::collect::CollectedFn;

#[instrument(level = "debug", skip(collected))]
pub(super) fn never_instrument_from_calls(
    collected: &[CollectedFn],
) -> HashMap<String, HashSet<String>> {
    let registry = build_registry(collected);
    let callers_of = build_callers_of(collected, &registry);
    let excluded = proof_only_fixed_point(collected, &callers_of);
    excluded
        .into_iter()
        .fold(HashMap::new(), |mut never_instrument, index| {
            let id = collected[index].id();
            never_instrument
                .entry(id.crate_name().clone())
                .or_insert_with(HashSet::new)
                .insert(id.qualified_name().clone());
            never_instrument
        })
}

/// Registry: call key -> every collected function whose own
/// call_keys include it (workspace-wide, deliberately not crate-
/// scoped -- a call site's own text doesn't name which crate its
/// callee lives in).
#[instrument(level = "debug", skip(collected))]
fn build_registry<'a>(collected: &'a [CollectedFn<'_>]) -> HashMap<&'a str, Vec<usize>> {
    let mut registry: HashMap<&str, Vec<usize>> = HashMap::new();
    for (index, function) in collected.iter().enumerate() {
        for key in function.facts().call_keys() {
            registry.entry(key.as_str()).or_default().push(index);
        }
    }
    registry
}

#[instrument(level = "debug", skip(collected, registry))]
fn build_callers_of(
    collected: &[CollectedFn],
    registry: &HashMap<&str, Vec<usize>>,
) -> HashMap<usize, HashSet<usize>> {
    let mut callers_of: HashMap<usize, HashSet<usize>> = HashMap::new();
    for (caller_index, function) in collected.iter().enumerate() {
        for key in function.facts().calls() {
            let Some(candidates) = registry.get(key.as_str()) else {
                continue;
            };
            // Ambiguous (>1 workspace-wide definition sharing this
            // exact key) -- don't guess which one a bare/short name
            // meant. This registry does not establish the absence of
            // unresolved or external callers.
            let [callee_index] = candidates[..] else {
                continue;
            };
            if callee_index != caller_index {
                callers_of
                    .entry(callee_index)
                    .or_default()
                    .insert(caller_index);
            }
        }
    }
    callers_of
}

#[instrument(level = "debug", skip(collected, callers_of))]
fn proof_only_fixed_point(
    collected: &[CollectedFn],
    callers_of: &HashMap<usize, HashSet<usize>>,
) -> HashSet<usize> {
    let mut excluded: HashSet<usize> = collected
        .iter()
        .enumerate()
        .filter(|(_, function)| function.ancestor_seed())
        .map(|(index, _)| index)
        .collect();
    loop {
        let mut added_any = false;
        for (index, callers) in callers_of {
            if excluded.contains(index) {
                continue;
            }
            if !callers.is_empty() && callers.iter().all(|caller| excluded.contains(caller)) {
                excluded.insert(*index);
                added_any = true;
            }
        }
        if !added_any {
            break;
        }
    }
    excluded
}

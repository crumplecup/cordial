//! Derive reusable failure alternatives from shared source and call evidence.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::error::CordialResult;
use crate::hooks::{EnrichView, IrEnricher};
use crate::ir::{
    ATTR_FUNCTION_ERROR_FAMILY, BasicQuery, ErrorFamilyBasis, ErrorFamilyGap, ErrorFamilyGapReason,
    ErrorFamilyMember, FunctionErrorFamily, FunctionSourceSyntax, IrView, NodeId, SourceByteRange,
};

mod collect;

/// Compute a fixed point once, then persist every summary on its function node.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ErrorFamilyEnricher;

impl IrEnricher for ErrorFamilyEnricher {
    #[tracing::instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        "error-family"
    }

    #[tracing::instrument(level = "trace", skip(self))]
    fn priority(&self) -> u8 {
        3
    }

    #[tracing::instrument(level = "trace", skip(self, view), err(level = "warn"))]
    #[hotpath::measure(impl_type = "ErrorFamilyEnricher")]
    fn enrich(&self, view: EnrichView<'_>) -> CordialResult<()> {
        let ir = view.ir();
        let mut functions = BTreeMap::new();
        for node in ir.nodes_matching(&BasicQuery::all_nodes()) {
            if let Some(facts) = ir.function_source_syntax(node.id())? {
                functions.insert(node.id().value(), facts);
            }
        }
        let mut states = BTreeMap::new();
        for (&id, facts) in &functions {
            states.insert(
                id,
                collect::collect(ir, NodeId::new(id), facts, &functions)?,
            );
        }
        propagate(&mut states);
        for (id, state) in states {
            let members = state
                .members
                .into_iter()
                .map(|(identity, origins)| {
                    ErrorFamilyMember::new(identity, origins.into_iter().map(NodeId::new).collect())
                })
                .collect();
            let gaps = state
                .gaps
                .into_iter()
                .map(|(function, range, reason)| {
                    ErrorFamilyGap::new(
                        NodeId::new(function),
                        range.map(|(start, end)| (start..end).into()),
                        reason,
                    )
                })
                .collect();
            ir.set_attr(
                NodeId::new(id),
                ATTR_FUNCTION_ERROR_FAMILY,
                serde_json::to_value(FunctionErrorFamily::new(
                    state.digest,
                    state.basis,
                    members,
                    gaps,
                ))?,
            )?;
        }
        Ok(())
    }
}

type GapKey = (u32, Option<(usize, usize)>, ErrorFamilyGapReason);

struct FamilyState {
    digest: [u8; 32],
    basis: ErrorFamilyBasis,
    members: BTreeMap<String, BTreeSet<u32>>,
    gaps: BTreeSet<GapKey>,
    dependencies: BTreeSet<u32>,
}

impl FamilyState {
    #[tracing::instrument(level = "trace", skip(self, function, range, reason))]
    fn gap(
        &mut self,
        function: NodeId,
        range: Option<&SourceByteRange>,
        reason: ErrorFamilyGapReason,
    ) {
        self.gaps.insert((
            function.value(),
            range.map(|range| (range.start(), range.end())),
            reason,
        ));
    }
}

/// Monotone sets terminate even for recursive components. Reverse adjacency
/// revisits only dependants whose inputs changed, rather than scanning the
/// whole crate on every round. These temporary work sets are published to IR.
#[tracing::instrument(level = "debug", skip(states))]
#[hotpath::measure]
fn propagate(states: &mut BTreeMap<u32, FamilyState>) {
    let mut reverse: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for (&id, state) in states.iter() {
        for target in &state.dependencies {
            reverse.entry(*target).or_default().push(id);
        }
    }
    let mut queue: VecDeque<_> = states.keys().copied().collect();
    let mut queued: BTreeSet<_> = states.keys().copied().collect();
    while let Some(target) = queue.pop_front() {
        queued.remove(&target);
        let members = states[&target].members.clone();
        let gaps = states[&target].gaps.clone();
        for caller in reverse.get(&target).into_iter().flatten() {
            let Some(state) = states.get_mut(caller) else {
                continue;
            };
            let mut changed = false;
            for (identity, origins) in &members {
                let known = state.members.entry(identity.clone()).or_default();
                let before = known.len();
                known.extend(origins);
                changed |= known.len() != before;
            }
            let before = state.gaps.len();
            state.gaps.extend(gaps.iter().copied());
            changed |= state.gaps.len() != before;
            if changed && queued.insert(*caller) {
                queue.push_back(*caller);
            }
        }
    }
}

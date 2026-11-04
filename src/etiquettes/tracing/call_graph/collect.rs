//! Project shared workspace evidence into policy-specific graph vertices.

use super::super::apply::crate_gate_cfgs;
use crate::PathInclusionFacts;
use crate::config::TracingThresholds;
use crate::ir::SourceProofFunction;

#[derive(Debug, Clone, PartialEq, Eq, Hash, derive_getters::Getters)]
pub(super) struct FunctionId {
    crate_name: String,
    qualified_name: String,
}

#[derive(derive_getters::Getters)]
pub(super) struct CollectedFn<'a> {
    id: FunctionId,
    facts: &'a SourceProofFunction,
    #[getter(copy)]
    ancestor_seed: bool,
}

#[tracing::instrument(level = "debug", skip(config, path_facts))]
pub(super) fn collect_workspace_functions<'a>(
    config: &TracingThresholds,
    path_facts: &'a PathInclusionFacts,
) -> Vec<CollectedFn<'a>> {
    let mut collected = Vec::new();
    for (crate_name, _) in path_facts.crate_roots() {
        let gates = crate_gate_cfgs(crate_name, config, path_facts);
        for source in path_facts
            .sources()
            .iter()
            .filter(|source| source.crate_name() == crate_name)
        {
            for facts in source.proof_calls() {
                collected.push(CollectedFn {
                    id: FunctionId {
                        crate_name: crate_name.to_owned(),
                        qualified_name: facts.qualified_name().clone(),
                    },
                    ancestor_seed: facts
                        .cfg_names()
                        .iter()
                        .chain(facts.attribute_names())
                        .any(|name| gates.contains(name)),
                    facts,
                });
            }
        }
    }
    collected
}

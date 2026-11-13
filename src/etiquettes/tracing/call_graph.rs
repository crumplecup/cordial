//! Workspace-wide call-graph facts: which functions are reachable *only* from
//! proof-only code -- code that exists only in a verifier's build -- so that
//! instrumenting them can never produce observable output in any real build,
//! exactly like the proof-only code itself.
//!
//! **Evidence.** Everything here is read from the crate IRs the loader built
//! from the one session database: resolved `Calls` and `ContextCalls` edges (the
//! latter are calls that exist only under a cfg context such as `kani`), calls
//! into other workspace crates matched by source location, calls through a
//! trait followed to each implementation, and functions read from macro
//! invocations whose calls were resolved at the invocation. No name is matched
//! against another name.
//!
//! **Seeds.** A function is proof-only outright when it is inactive in the
//! ordinary build and active only in contexts that turn on one of its crate's
//! gate cfgs (see [`crate_gate_cfgs`](super::apply::crate_gate_cfgs)), or when
//! a function read from a macro names such a gate.
//!
//! **Fixed point.** Then repeatedly add any function whose callers are **all**
//! already excluded (and it has at least one) until nothing changes. A function
//! with no known caller -- `pub` API an external crate might call, or genuinely
//! dead code -- is never added: there is no positive evidence it is proof-only,
//! and gating dead code costs nothing either way. Neither is a function that
//! implements a trait the workspace does not own: the standard library or a
//! dependency may call it where nothing here can see.

use std::collections::{HashMap, HashSet};

use crate::config::TracingThresholds;
use crate::{CordialResult, CrateIr, PathInclusionFacts};

use tracing::instrument;

mod exclusion;
mod graph;

use super::apply::crate_gate_cfgs;
use graph::Origin;

/// Workspace-wide call-graph facts retained by the session IR.
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct CallGraphFacts {
    /// Crate name -> qualified names never worth recording as needing
    /// `#[instrument]`, regardless of role/recipe.
    never_instrument: HashMap<String, HashSet<String>>,
}

static EMPTY: std::sync::OnceLock<HashSet<String>> = std::sync::OnceLock::new();

impl CallGraphFacts {
    /// Qualified names in `crate_name` that are reachable only from
    /// proof-only code -- never worth recording as needing `#[instrument]`.
    #[instrument(level = "trace", skip(self))]
    pub fn never_instrument(&self, crate_name: &str) -> &HashSet<String> {
        self.never_instrument
            .get(crate_name)
            .unwrap_or_else(|| EMPTY.get_or_init(HashSet::new))
    }
}

/// Evaluate proof-only reachability over the workspace's crate IRs.
#[instrument(level = "debug", skip(config, path_facts, crates), err(level = "warn"))]
pub(crate) fn workspace_call_graph(
    config: &TracingThresholds,
    path_facts: &PathInclusionFacts,
    crates: &HashMap<String, CrateIr>,
) -> CordialResult<CallGraphFacts> {
    let graph = graph::build(crates)?;
    let gates: HashMap<String, _> = crates
        .keys()
        .map(|name| (name.clone(), crate_gate_cfgs(name, config, path_facts)))
        .collect();
    let mut never_instrument: HashMap<String, HashSet<String>> = HashMap::new();
    for vertex in exclusion::proof_only(&graph, &gates) {
        let function = &graph.vertices()[vertex];
        if matches!(function.origin(), Origin::Source(_)) {
            never_instrument
                .entry(function.crate_name().clone())
                .or_default()
                .insert(function.qualified_name().clone());
        }
    }
    Ok(CallGraphFacts { never_instrument })
}

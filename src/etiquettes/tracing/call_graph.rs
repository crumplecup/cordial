//! Workspace-wide call-graph facts: which functions are reachable
//! *only* from proof-only entry points -- functions nested inside an
//! ancestor `#[cfg(<verifier-cfg>)]`, the crate's own gate cfg name(s)
//! (see [`crate_gate_cfgs`](super::apply::crate_gate_cfgs)) -- so
//! instrumenting them can never produce observable output in any real
//! build, exactly like the entry points themselves.
//!
//! **Why this has to be call-graph-based, not trait-name-based.** A
//! first version of this recognized `amenable_core::Ensures`/`Requires`
//! impls by name specifically -- real, but a special case tied to one
//! workspace's own trait names, not a mechanism any other cordial user
//! gets for free. The actual, reusable invariant is call-graph
//! reachability: a function whose *every* real caller, transitively,
//! bottoms out in a proof-only entry point is exactly as dead-to-
//! tracing as the entry point itself, whatever trait (if any) it
//! happens to implement.
//!
//! **How.** No rustc integration, so no real type inference -- call
//! resolution is name-based: `Type::method(..)`/`bare_fn(..)` (explicit
//! path syntax) is unambiguous enough to match against a workspace-wide
//! registry of known function definitions by their own last one or two
//! path segments; `receiver.method(..)` calls, whose receiver's type
//! isn't known without real inference, are not resolved at all and so
//! never produce a graph edge -- a missed edge only risks under-
//! excluding (a function stays `Gated` that could have been `Skip`),
//! never the reverse. Fixed point: seed `excluded` with every function
//! nested in an ancestor gate cfg, then repeatedly add any function
//! whose in-workspace callers are **all** already `excluded` (and it
//! has at least one) until nothing changes. A function with zero known
//! in-workspace callers -- `pub` API an external crate might call, or
//! genuinely dead code -- is never added: no positive evidence it's
//! proof-only, and gating dead code costs nothing either way.
//!
//! Scoped to same-workspace calls only (no attempt to resolve a call
//! into an external, non-workspace dependency) -- every real case this
//! was built against (`amenable_kani`'s own `Ensures`/`Requires`
//! impls, called from its own proof harnesses) is intra-workspace.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::config::TracingThresholds;
use crate::{PathInclusionFacts, workspace_path_inclusions};

use tracing::instrument;

mod collect;
mod resolve;

use collect::collect_workspace_functions;
use resolve::never_instrument_from_calls;

/// Workspace-wide call-graph facts, computed once per session (cached
/// like [`PathInclusionFacts`]).
#[derive(Debug, Default, Clone)]
pub struct CallGraphFacts {
    /// Crate name -> qualified names never worth recording as needing
    /// `#[instrument]`, regardless of role/recipe.
    never_instrument: HashMap<String, HashSet<String>>,
}

static EMPTY: std::sync::OnceLock<HashSet<String>> = std::sync::OnceLock::new();

impl CallGraphFacts {
    /// Qualified names in `crate_name` that are reachable only from
    /// proof-only entry points -- never worth recording as needing
    /// `#[instrument]`.
    #[instrument(level = "trace", skip(self))]
    pub fn never_instrument(&self, crate_name: &str) -> &HashSet<String> {
        self.never_instrument
            .get(crate_name)
            .unwrap_or_else(|| EMPTY.get_or_init(HashSet::new))
    }
}

type FactsCache = Mutex<Option<(PathBuf, CallGraphFacts)>>;
static FACTS_CACHE: FactsCache = Mutex::new(None);

/// Cached, workspace-wide call-graph facts -- computed once per
/// `workspace_root` per session, matching
/// [`crate::workspace_path_inclusions`]'s own cache shape.
#[instrument(level = "debug", skip(config))]
pub fn workspace_call_graph(workspace_root: &Path, config: &TracingThresholds) -> CallGraphFacts {
    let cache_key = workspace_root
        .canonicalize()
        .unwrap_or_else(|_| workspace_root.to_path_buf());
    if let Ok(cache) = FACTS_CACHE.lock()
        && let Some((key, facts)) = cache.as_ref()
        && *key == cache_key
    {
        return facts.clone();
    }

    let path_facts = workspace_path_inclusions(workspace_root);
    let facts = compute_call_graph(config, &path_facts);
    if let Ok(mut cache) = FACTS_CACHE.lock() {
        *cache = Some((cache_key, facts.clone()));
    }
    facts
}

#[instrument(level = "debug", skip(config, path_facts))]
fn compute_call_graph(
    config: &TracingThresholds,
    path_facts: &PathInclusionFacts,
) -> CallGraphFacts {
    let collected = collect_workspace_functions(config, path_facts);
    let never_instrument = never_instrument_from_calls(&collected);
    CallGraphFacts { never_instrument }
}

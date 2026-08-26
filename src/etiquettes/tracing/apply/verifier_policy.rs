//! Per-file tracing-apply policy: whether `#[instrument(..)]` is safe to
//! write bare, needs a `cfg_attr` gate, or must be skipped entirely,
//! given every real compilation unit that ends up compiling this file's
//! text.
//!
//! **Why a file can compile under more than one crate's own rules.**
//! Two distinct, real mechanisms widen a file's real "who compiles this"
//! set beyond its own owning crate: an ordinary Cargo dependency edge
//! (crate `B` depends on crate `A`, so `A`'s source compiles as part of
//! `B`'s own build, under whatever global compiler flags that build
//! uses -- `cargo kani`'s `--cfg kani` is one such flag, applied to the
//! *whole* dependency graph it compiles, not just the top-level target
//! crate), and a `#[path]`-splice (crate `B` names `A`'s file directly
//! via `#[path = ".."] mod foo;`, so the file's real text is
//! *re-compiled from scratch* as part of `B`'s own compilation unit,
//! independent of `B`'s Cargo dependency list -- see
//! [`crate::PathInclusionFacts`]'s own doc comment for the real
//! precedent this was built against).
//!
//! **Skip does not propagate the same way Gate does.** A dependency-
//! graph-wide compiler flag (Kani's `--cfg kani`) reaches every crate in
//! the graph, so gating has to propagate transitively through ordinary
//! dependencies. A translator that only sweeps a crate's own local items
//! (real precedent: `creusot-rustc`, confirmed empirically -- it never
//! ICEs on an ordinary dependency's own source) has no reason to touch
//! anything outside the skip-configured crate itself, so skip does
//! *not* propagate through ordinary dependencies. It does propagate
//! through a `#[path]` splice, though, since that's a second, distinct
//! compilation of the exact same file text, this time as if it were
//! local to the splicing crate.

use std::collections::BTreeSet;
use std::path::Path;

use crate::{PathInclusionFacts, TracingThresholds};

/// What `--apply` should do with one function's `#[instrument(..)]`
/// insertion, given every crate that really compiles the file it lives
/// in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TracingApplyPolicy {
    /// Write `#[instrument(..)]` exactly as generated today.
    Bare,
    /// Write `#[cfg_attr(not(#cfg), instrument(..))]` -- or, for more
    /// than one distinct gate name, `#[cfg_attr(not(any(#(cfg),*)),
    /// instrument(..))]`. Sorted, deduplicated, always non-empty.
    Gated(Vec<String>),
    /// Do not write anything; the checklist item stays open.
    Skip,
}

/// Resolve the real tracing-apply policy for one file, given every
/// crate whose own compilation actually includes its text.
pub fn resolve_tracing_apply_policy(
    crate_name: &str,
    file: &Path,
    crate_root: &Path,
    config: &TracingThresholds,
    facts: &PathInclusionFacts,
) -> TracingApplyPolicy {
    let splice_consumers = facts.splice_consumers(file, crate_root);

    // Skip: this crate itself, or any crate that splices this exact
    // file's text in directly. Deliberately *not* widened through the
    // ordinary dependency graph -- see the module doc comment.
    let skip_consumers: BTreeSet<&str> = std::iter::once(crate_name)
        .chain(splice_consumers.iter().copied())
        .collect();
    if skip_consumers.iter().any(|consumer| {
        config
            .apply_skip_crates()
            .iter()
            .any(|skip| skip == consumer)
    }) {
        return TracingApplyPolicy::Skip;
    }

    // Gate: this crate (and its transitive dependents), plus every
    // splice consumer (and *their* transitive dependents) -- a real
    // compiler flag set for a whole dependency-graph build reaches
    // anything that build pulls in, however it got pulled in.
    let mut gate_cfgs = crate_gate_cfgs(crate_name, config, facts);
    for consumer in &splice_consumers {
        gate_cfgs.extend(crate_gate_cfgs(consumer, config, facts));
    }

    if gate_cfgs.is_empty() {
        TracingApplyPolicy::Bare
    } else {
        TracingApplyPolicy::Gated(gate_cfgs.into_iter().collect())
    }
}

/// Every gate cfg name applicable to `crate_name`'s own compilation --
/// its own `apply_gate_crates` entry, plus any transitive dependent's
/// (a real compiler flag set for a whole dependency-graph build reaches
/// anything that build pulls in). Deliberately excludes `#[path]`-splice
/// consumers, unlike [`resolve_tracing_apply_policy`]'s own gate-cfg
/// computation -- this is "what cfg names make an item unreachable
/// *within `crate_name`'s own source*," not "what governs a file `crate_
/// name` happens to also compile under a different crate's rules."
pub(crate) fn crate_gate_cfgs(
    crate_name: &str,
    config: &TracingThresholds,
    facts: &PathInclusionFacts,
) -> BTreeSet<String> {
    std::iter::once(crate_name.to_owned())
        .chain(facts.transitive_dependents(crate_name))
        .filter_map(|candidate| config.apply_gate_crates().get(&candidate).cloned())
        .collect()
}

/// Render the `not(..)` predicate for [`TracingApplyPolicy::Gated`]'s
/// cfg list: `kani` for one name, `any(creusot, kani)` for more than
/// one.
pub fn gate_predicate(cfgs: &[String]) -> String {
    match cfgs {
        [only] => only.clone(),
        many => format!("any({})", many.join(", ")),
    }
}

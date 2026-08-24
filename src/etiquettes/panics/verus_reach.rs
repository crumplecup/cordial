//! Crate-local "verification leaf" detection for `verus! { .. }`
//! functions.
//!
//! Kani's proof harnesses have a real root (`#[kani::proof]`) that other
//! code is reachable *from* -- `kani_reach`'s own concept. This
//! codebase's Verus gallery-style `verify_*` functions have no such
//! root: they're never called by anything else at all, checked directly
//! by the real `verus` toolchain against their own `ensures` clause.
//! Confirmed empirically, not assumed: every real `amenable_verus` site
//! this was built for has zero other references anywhere in the crate.
//!
//! A function is a verification leaf when it carries a real `ensures`
//! clause (something Verus actually checks the body against) and no
//! other local function calls it by name -- the function itself IS the
//! checked claim, not library API surface serving other code. Every
//! panic-family site inside such a function is that verification's own
//! failure mechanism, the same "must never be flagged" reasoning
//! `kani_reach` applies to a Kani harness's own panic -- just keyed on
//! in-degree zero plus a real postcondition instead of reachability
//! from a `#[kani::proof]` root, since Verus has no analogous root to
//! be reachable *from*.

use std::collections::HashSet;

use crate::verus_ir::VerusCrateIr;

use tracing::instrument;

/// Crate-local set of verification-leaf function names -- see the
/// module doc.
#[derive(Debug, Default)]
pub(super) struct VerusReachability {
    leaves: HashSet<String>,
}

impl VerusReachability {
    #[instrument(level = "trace", skip(self, name), ret)]
    pub(super) fn is_verification_leaf(&self, name: &str) -> bool {
        self.leaves.contains(name)
    }
}

/// Build the verification-leaf set from a crate's real `verus_ir` facts.
#[instrument(level = "debug", skip(ir))]
pub(super) fn build_verus_reachability(ir: &VerusCrateIr) -> VerusReachability {
    let called: HashSet<&str> = ir
        .functions
        .iter()
        .flat_map(|function| function.calls.iter().map(String::as_str))
        .collect();

    let leaves = ir
        .functions
        .iter()
        .filter(|function| !function.ensures.is_empty())
        .filter(|function| !called.contains(function.name.as_str()))
        .map(|function| function.name.clone())
        .collect();

    VerusReachability { leaves }
}

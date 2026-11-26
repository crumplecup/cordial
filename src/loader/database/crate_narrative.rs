//! What the crates of a run have at the moments a flipped run is read: before the contexts, in a context, and after the
//! cfg is put back.
//!
//! A flipped context writes its cfg into the crates the pass reaches, and not the others, and a file that several crates
//! have is read through the first of them. When what one crate has is asked of another's cfg, or a call of an attribute
//! macro that a cfg turned off is still asked for, the log has to say which crates there were, which of them were
//! flipped, what cfg each had, and which attribute macro calls the map of each declared, so that the answer is a join of
//! lines and not another run. It is said outside any query, so that saying it adds no dependency to any memo.

use std::collections::BTreeSet;

use ra_ap_hir::attach_db;
use ra_ap_ide_db::base_db::{Crate as InputCrate, CrateOrigin, all_crates};
use salsa::plumbing::AsId as _;
use tracing::instrument;

use super::SourceDatabase;

/// The atoms of a crate's cfg that say what it is built for, without the ones every crate has.
fn flags_of(cfg: &str) -> String {
    cfg.split('"')
        .skip(1)
        .step_by(2)
        .filter(|atom| !(atom.starts_with("target_") || atom.starts_with("panic") || *atom == "rust_analyzer"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The attribute macro calls each crate declared before the contexts, to say what a context removed and added, and
/// whether putting the cfg back gives the same ones.
static ORDINARY_CALLS: std::sync::Mutex<Option<std::collections::HashMap<u64, std::collections::HashSet<u64>>>> =
    std::sync::Mutex::new(None);

impl SourceDatabase {
    /// Say, for each local crate, what it has now: its cfg flags, whether the context flipped it (`flipped` is the crates
    /// it did, and `None` before and after the contexts), and the numbers of the attribute macro calls its definition
    /// map declares. `phase` says when.
    #[instrument(level = "debug", skip(self, flipped))]
    pub(super) fn say_the_crates(&self, phase: &str, flipped: Option<&BTreeSet<InputCrate>>) {
        // The reachable facts say each function they go on to read while a context is flipped, and not before or after.
        super::queries::say_the_walk_is_flipped(flipped.is_some());
        if !tracing::enabled!(target: "cordial::semantic::flip", tracing::Level::DEBUG) {
            return;
        }
        attach_db(&self.db, || {
            for krate in all_crates(&self.db).iter() {
                if !matches!(krate.data(&self.db).origin, CrateOrigin::Local { .. }) {
                    continue;
                }
                let name = ra_ap_hir::Crate::from(*krate)
                    .display_name(&self.db)
                    .map_or_else(|| "?".to_owned(), |name| name.to_string());
                let map = ra_ap_hir_def::nameres::crate_def_map(&self.db, *krate);
                let mut modules = 0u64;
                let mut calls: Vec<u64> = Vec::new();
                for (_, module) in map.modules(&self.db) {
                    modules += 1;
                    calls.extend(module.scope.attr_macro_invocs().map(|(_, call)| call.as_id().as_bits()));
                }
                calls.sort_unstable();
                // Against the calls the crate declared before the contexts (said when there were none, and kept).
                let id = krate.as_id().as_bits();
                let now: std::collections::HashSet<u64> = calls.iter().copied().collect();
                let (removed, added) = {
                    let mut ordinary = ORDINARY_CALLS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                    let ordinary = ordinary.get_or_insert_with(Default::default);
                    if phase == "ordinary" {
                        ordinary.insert(id, now.clone());
                        (0, 0)
                    } else {
                        let before = ordinary.get(&id).cloned().unwrap_or_default();
                        (before.difference(&now).count() as u64, now.difference(&before).count() as u64)
                    }
                };
                tracing::debug!(
                    target: "cordial::semantic::flip",
                    phase,
                    krate = %name,
                    krate_id = format_args!("{:#x}", krate.as_id().as_bits()),
                    flipped = flipped.map(|flipped| flipped.contains(krate)),
                    flags = %flags_of(&format!("{:?}", krate.cfg_options(&self.db))),
                    modules,
                    attr_macro_calls = calls.len() as u64,
                    calls_removed_since_ordinary = removed,
                    calls_added_since_ordinary = added,
                    calls = %calls.iter().map(|call| format!("{call:#x}")).collect::<Vec<_>>().join(" "),
                    "what a crate has: its cfg, whether the context flipped it, and the attribute macro calls its map declares"
                );
            }
        });
    }
}

//! A cfg context as the ordinary crates under changed cfg, flipped in place.
//!
//! A context used to be copies of the crates under their own files (`context_crates`), so that what the ordinary
//! build computed stayed valid beside it. Rust-analyzer now asks the cfg one atom at a time and a finished
//! definition map is read by module (`docs/planning/contexts-as-flips.md`), so a crate's own cfg can change and
//! only what read a flipped atom runs again. A context writes its change, is read from the same crates and files,
//! and the ordinary cfg is written back.

use super::LocalCfgs;
use crate::SemanticContextKey;
use crate::loader::database::SourceDatabase;
use crate::loader::database::context_crates::ContextCrates;
use ra_ap_cfg::CfgOptions;
use ra_ap_ide_db::base_db::Crate as InputCrate;
use ra_ap_hir::attach_db;
use ra_ap_hir_def::nameres::stable::module_fingerprints;
use salsa::Durability;
use salsa::Setter as _;
use std::collections::HashMap;
use tracing::instrument;

/// What each module of each local crate said in the ordinary build, to say how many a context's cfg changed.
pub(super) struct Baseline(HashMap<InputCrate, HashMap<u64, u64>>);

impl SourceDatabase {
    /// Give each local crate the cfg `context` makes of its ordinary one, and say which crates they are.
    #[instrument(level = "debug", skip(self, cfgs))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn flip_context(
        &mut self,
        cfgs: &LocalCfgs,
        context: &SemanticContextKey,
    ) -> ContextCrates {
        let pairs = cfgs.pairs(context);
        let mut flipped = 0usize;
        let mut crates = Vec::new();
        for ((krate, _), (_, changed)) in cfgs.cfgs().zip(pairs) {
            flipped += usize::from(self.write_cfg(*krate, changed));
            crates.push(*krate);
        }
        tracing::info!(
            target: "cordial::semantic::contexts",
            %context,
            crates = crates.len() as u64,
            flipped = flipped as u64,
            "a context's cfg is written into the ordinary crates"
        );
        ContextCrates::in_place(context.clone(), crates)
    }

    /// Put each local crate's ordinary cfg back.
    #[instrument(level = "debug", skip(self, cfgs))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn restore_cfgs(&mut self, cfgs: &LocalCfgs) {
        for (krate, ordinary) in cfgs.cfgs() {
            self.write_cfg(*krate, ordinary.clone());
        }
    }

    /// What the modules of the local crates say now, when the run records what a definition map cost.
    #[instrument(level = "debug", skip(self, cfgs))]
    pub(super) fn module_baseline(&self, cfgs: &LocalCfgs) -> Option<Baseline> {
        tracing::enabled!(target: "cordial::def_map", tracing::Level::INFO).then(|| {
            Baseline(
                cfgs.cfgs()
                    .map(|(krate, _)| (*krate, self.module_hashes(*krate)))
                    .collect(),
            )
        })
    }

    fn module_hashes(&self, krate: InputCrate) -> HashMap<u64, u64> {
        attach_db(&self.db, || module_fingerprints(&self.db, krate).into_iter().collect())
    }

    /// Say how many modules of the local crates the flipped cfg changed, added and removed against the ordinary
    /// ones: the definition work that was needed, whatever was done.
    #[instrument(level = "debug", skip(self, baseline))]
    pub(super) fn audit_flip(&self, baseline: &Baseline, context: &SemanticContextKey) {
        let (mut modules, mut changed, mut added, mut removed, mut crates) = (0u64, 0u64, 0u64, 0u64, 0u64);
        for (krate, before) in &baseline.0 {
            let after = self.module_hashes(*krate);
            let different = after
                .iter()
                .filter(|(key, hash)| before.get(key).is_some_and(|old| old != *hash))
                .count() as u64;
            let new = after.keys().filter(|key| !before.contains_key(key)).count() as u64;
            let gone = before.keys().filter(|key| !after.contains_key(key)).count() as u64;
            modules += after.len() as u64;
            changed += different;
            added += new;
            removed += gone;
            crates += u64::from(different + new + gone > 0);
        }
        tracing::info!(
            target: "cordial::def_map",
            %context,
            modules,
            modules_changed = changed,
            modules_added = added,
            modules_removed = removed,
            crates_changed = crates,
            "the definition maps of a flipped context against the ordinary ones"
        );
    }

    /// Write `options` as the crate's cfg when it is not what it holds: an input is not compared to its old value,
    /// so writing an equal one would make everything that read it stale. Whether it was written.
    fn write_cfg(&mut self, krate: InputCrate, options: CfgOptions) -> bool {
        if *krate.cfg_options(&self.db) == options {
            return false;
        }
        krate
            .set_cfg_options(&mut self.db)
            .with_durability(Durability::LOW)
            .to(options);
        true
    }
}

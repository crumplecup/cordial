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
use crate::loader::database::context_crates::functions_of;
use crate::loader::database::item_key::key_of;
use crate::loader::database::queries::FunctionKey;
use ra_ap_hir::{Crate, Function, ModuleDef, attach_db};
use salsa::plumbing::AsId as _;
use ra_ap_hir_def::nameres::crate_declared_query_keys;
use ra_ap_hir_def::nameres::stable::module_fingerprints;
use salsa::Durability;
use salsa::Setter as _;
use std::collections::HashMap;
use tracing::instrument;

/// What the local crates were in the ordinary build: the items each declared, and (when the run records what a
/// definition map cost) what each module said, so that a context can be told from it.
pub(super) struct Ordinary {
    /// Every function the local crates declare.
    functions: std::collections::HashSet<Function>,
    declared: HashMap<InputCrate, std::collections::HashSet<salsa::Id>>,
    modules: Option<HashMap<InputCrate, HashMap<u64, u64>>>,
}

impl Ordinary {
    /// What each module of each local crate said, when that was recorded.
    pub(super) fn modules(&self) -> Option<&HashMap<InputCrate, HashMap<u64, u64>>> {
        self.modules.as_ref()
    }
}

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

    /// Make the local crates' definition maps again under the ordinary cfg, on this thread. A map made again
    /// deletes the modules the flip added, and the threads that verify what is saved would otherwise make it
    /// again among themselves, deleting a module that another is reading.
    #[instrument(level = "debug", skip(self, cfgs))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn settle_ordinary(&self, cfgs: &LocalCfgs) {
        let started = std::time::Instant::now();
        attach_db(&self.db, || {
            for (krate, _) in cfgs.cfgs() {
                let _ = ra_ap_hir_def::nameres::crate_def_map(&self.db, *krate);
            }
        });
        tracing::info!(
            target: "cordial::def_map",
            crates = cfgs.cfgs().count() as u64,
            micros = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX),
            "the definition maps of the local crates are made again under the ordinary cfg"
        );
    }

    /// Put each local crate's ordinary cfg back.
    #[instrument(level = "debug", skip(self, cfgs))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn restore_cfgs(&mut self, cfgs: &LocalCfgs) {
        for (krate, ordinary) in cfgs.cfgs() {
            self.write_cfg(*krate, ordinary.clone());
        }
    }

    /// The local crates as they are now, which is the ordinary build.
    #[instrument(level = "debug", skip(self, cfgs))]
    pub(super) fn ordinary_state(&mut self, cfgs: &LocalCfgs) -> Ordinary {
        let declared = cfgs
            .cfgs()
            .map(|(krate, _)| {
                let keys = crate_declared_query_keys(&mut self.db, *krate);
                (*krate, keys.into_iter().collect())
            })
            .collect();
        let modules = tracing::enabled!(target: "cordial::def_map", tracing::Level::INFO).then(|| {
            cfgs.cfgs()
                .map(|(krate, _)| (*krate, self.module_hashes(*krate)))
                .collect()
        });
        let functions = self.local_functions(cfgs);
        Ordinary { functions, declared, modules }
    }

    /// Every function the local crates declare now.
    fn local_functions(&self, cfgs: &LocalCfgs) -> std::collections::HashSet<Function> {
        let locals: std::collections::HashSet<InputCrate> =
            cfgs.cfgs().map(|(krate, _)| *krate).collect();
        attach_db(&self.db, || {
            Crate::all(&self.db)
                .into_iter()
                .filter(|krate| locals.contains(&krate.base()))
                .flat_map(|krate| functions_of(&self.db, krate))
                .collect()
        })
    }

    /// What the flipped crates declare that the ordinary build does not: items that exist only in this context.
    /// A memo keyed by one runs against a module or an item that the ordinary cfg does not have, so it is not
    /// left to be verified when the state is saved (see `Database::forget_memos_for_inputs`).
    #[instrument(level = "debug", skip(self, ordinary))]
    pub(super) fn declared_only_when_flipped(
        &mut self,
        cfgs: &LocalCfgs,
        ordinary: &Ordinary,
    ) -> Vec<salsa::Id> {
        let (mut only, mut now, mut before_total) = (Vec::new(), 0u64, 0u64);
        for (krate, before) in &ordinary.declared {
            let declared = crate_declared_query_keys(&mut self.db, *krate);
            now += declared.len() as u64;
            before_total += before.len() as u64;
            only.extend(declared.into_iter().filter(|key| !before.contains(key)));
        }
        // What the tool saved of those functions is keyed by the function and by its description, which a saved
        // state would verify against an ordinary cfg that has no such function.
        let functions_only_here: Vec<Function> = self
            .local_functions(cfgs)
            .into_iter()
            .filter(|function| !ordinary.functions.contains(function))
            .collect();
        let keys = attach_db(&self.db, || {
            let mut keys = Vec::new();
            for function in &functions_only_here {
                keys.push(FunctionKey::new(&self.db, *function).as_id());
                keys.push(key_of(&self.db, ModuleDef::Function(*function)).as_id());
            }
            keys
        });
        only.extend(keys);
        tracing::info!(
            target: "cordial::eviction",
            functions_only_flipped = functions_only_here.len() as u64,
            declared_flipped = now,
            declared_ordinary = before_total,
            only_flipped = only.len() as u64,
            "what the flipped crates declare that the ordinary build does not"
        );
        only
    }

    /// Forget what was computed for `items`, which the ordinary cfg does not have.
    #[instrument(level = "debug", skip(self, items))]
    pub(super) fn forget_flipped(&mut self, items: &[salsa::Id]) {
        let forgotten = salsa::Database::forget_memos_for_inputs(&mut self.db, items);
        tracing::info!(
            target: "cordial::eviction",
            items = items.len() as u64,
            forgotten,
            "memos of items only a flipped context has are forgotten"
        );
    }

    fn module_hashes(&self, krate: InputCrate) -> HashMap<u64, u64> {
        attach_db(&self.db, || module_fingerprints(&self.db, krate).into_iter().collect())
    }

    /// Say how many modules of the local crates the flipped cfg changed, added and removed against the ordinary
    /// ones: the definition work that was needed, whatever was done.
    #[instrument(level = "debug", skip(self, baseline))]
    pub(super) fn audit_flip(&self, baseline: &HashMap<InputCrate, HashMap<u64, u64>>, context: &SemanticContextKey) {
        let (mut modules, mut changed, mut added, mut removed, mut crates) = (0u64, 0u64, 0u64, 0u64, 0u64);
        for (krate, before) in baseline {
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

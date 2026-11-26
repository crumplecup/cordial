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

/// A pass over the contexts that flips the ordinary crates: what they were, what only the contexts so far have,
/// and the revision it began at. The cfg is written from one context straight to the next, so what a context
/// computed stays for the one after it; the ordinary cfg is written back, and what the flips changed is forgotten,
/// once, when the pass is done.
pub(super) struct FlipRun {
    ordinary: Ordinary,
    /// The items the contexts so far have and the ordinary build does not.
    only: std::collections::HashSet<salsa::Id>,
    start: salsa::Revision,
}

impl FlipRun {
    /// What each module of each local crate said in the ordinary build, when that was recorded.
    pub(super) fn modules(&self) -> Option<&HashMap<InputCrate, HashMap<u64, u64>>> {
        self.ordinary.modules()
    }
}

impl Ordinary {
    /// What each module of each local crate said, when that was recorded.
    pub(super) fn modules(&self) -> Option<&HashMap<InputCrate, HashMap<u64, u64>>> {
        self.modules.as_ref()
    }
}

/// What a block made by a macro is said to belong to, since it is in no file of its own.
const MACRO_OWNER: &str = "in what a macro made";

/// A flipped session: the crates it is over, with their ordinary cfgs, and what the flips have changed so far. The
/// contexts pass and the leaf pass are two parts of one session when the run can be, so that the ordinary cfg is put
/// back, what the flips changed forgotten and the maps settled once, when the last of them is done, and the second
/// part begins from the flipped state the first leaves.
pub(in crate::loader::database) struct FlipSession {
    pub(super) cfgs: LocalCfgs,
    pub(super) run: Option<FlipRun>,
}

impl SourceDatabase {
    /// Begin a flipped session over the crates the files in `paths` reach (and every crate cfg contexts are about, when
    /// contexts are copied and not flipped).
    #[instrument(level = "debug", skip(self, paths))]
    pub(in crate::loader::database) fn begin_session(&mut self, paths: &[std::path::PathBuf]) -> FlipSession {
        let reach = if self.flips_contexts() { self.crates_reaching(paths) } else { self.analysed_crates() };
        let cfgs = LocalCfgs::capture(&self.db, &reach);
        let run = self.begin_flips(&cfgs);
        FlipSession { cfgs, run }
    }

    /// End a session: the ordinary cfg is back, what the flips changed forgotten, and the maps settled.
    #[instrument(level = "debug", skip(self, session))]
    pub(in crate::loader::database) fn finish_session(&mut self, session: FlipSession) {
        if let Some(run) = session.run {
            self.finish_flips(&session.cfgs, run);
        }
    }

    /// Give each local crate the cfg `context` makes of its ordinary one, and say which crates they are.
    #[instrument(level = "debug", skip(self, cfgs))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn flip_context(
        &mut self,
        cfgs: &LocalCfgs,
        context: &SemanticContextKey,
    ) -> ContextCrates {
        let _write = crate::Segment::begin("flip_write");
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

    /// Make the local crates' definition maps again under the ordinary cfg, and the maps of the blocks that
    /// remain, on this thread. A map made again deletes the modules the flip added and makes the others again, and
    /// the threads that verify what is saved would otherwise do it among themselves, one deleting or updating a
    /// module that another is reading: a tracked struct must be made again in a revision before it is read in it.
    #[instrument(level = "debug", skip(self, cfgs))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn settle_ordinary(&mut self, cfgs: &LocalCfgs) {
        use ra_ap_hir_def::BlockId;
        use ra_ap_hir_def::declaration::settle_block_map;
        use salsa::plumbing::FromId as _;
        let _settle = crate::Segment::begin("settle");
        let started = std::time::Instant::now();
        tracing::info!(
            target: "cordial::def_map",
            "the definition maps and the bodies of the local crates are about to be made again under the ordinary cfg"
        );
        let blocks = {
            let _keys = crate::Segment::begin("block_keys");
            salsa::Database::memo_keys(&mut self.db, &["block_def_map"])
        };
        let revision = self.revision();
        let db = &self.db;
        let mut remade = 0u64;
        // Who each block that is held belongs to, while it can still be read: a block that a body made again without
        // is deleted, and nothing of it can be read after.
        let owners: Vec<(salsa::Id, String)> = attach_db(db, || {
            blocks.iter().map(|key| (*key, self.block_owner(BlockId::from_id(*key)))).collect()
        });
        attach_db(db, || {
            {
                let _maps = crate::Segment::begin("maps");
                for (krate, _) in cfgs.cfgs() {
                    let _ = ra_ap_hir_def::nameres::crate_def_map(db, *krate);
                }
            }
            // The bodies before the blocks: a body that is made again deletes the blocks it made before and does not
            // make again, and a block that was read in the revision cannot be deleted in it. Reading what a block is
            // of is reading it.
            {
                let _bodies = crate::Segment::begin("bodies");
                for (krate, _) in cfgs.cfgs() {
                    ra_ap_hir_def::declaration::settle_bodies(db, *krate);
                }
            }
            let gone: Vec<&str> = owners
                .iter()
                .filter(|(key, _)| salsa::Database::existence(db, *key) != salsa::Existence::Exists)
                .map(|(_, owner)| owner.as_str())
                .collect();
            tracing::info!(
                target: "cordial::def_map",
                revision = ?revision,
                blocks_held = owners.len() as u64,
                blocks_gone_after_bodies = gone.len() as u64,
                gone_in_macros = gone.iter().filter(|owner| **owner == MACRO_OWNER).count() as u64,
                gone_owners = %Self::owners_top(&gone),
                "the blocks the bodies made again no longer make: deleted before the blocks are read"
            );
            let _blocks = crate::Segment::begin("blocks");
            for key in blocks {
                settle_block_map(db, BlockId::from_id(key));
                remade += 1;
            }
        });
        tracing::info!(
            target: "cordial::def_map",
            crates = cfgs.cfgs().count() as u64,
            blocks = remade,
            micros = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX),
            "the definition maps of the local crates and of the blocks that remain are made again under the ordinary cfg"
        );
    }

    /// Put each local crate's ordinary cfg back.
    #[instrument(level = "debug", skip(self, cfgs))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn restore_cfgs(&mut self, cfgs: &LocalCfgs) {
        let _restore = crate::Segment::begin("restore");
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
        let _declared = crate::Segment::begin("declared_keys");
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

    /// Whether a context is analysed by flipping the ordinary crates' cfg, as the options say.
    pub(in crate::loader::database) fn flips_contexts(&self) -> bool {
        self.cargo_options
            .as_ref()
            .is_some_and(|options| *options.cfg_context_mode() == crate::CfgContextMode::Flips)
    }

    /// A pass over the contexts begins: the local crates as they are are the ordinary build, when the contexts are
    /// flipped.
    pub(super) fn begin_flips(&mut self, cfgs: &LocalCfgs) -> Option<FlipRun> {
        let _begin = crate::Segment::begin("begin_flips");
        if self.flips_contexts() {
            self.say_the_crates("ordinary", None);
        }
        self.flips_contexts().then(|| FlipRun {
            start: self.revision(),
            ordinary: self.ordinary_state(cfgs),
            only: std::collections::HashSet::new(),
        })
    }

    /// A context is finished. For a flip, what only it has is told before eviction takes the values its members
    /// are read from, and kept for the end of the pass: the cfg stays as it is, and the next context is written
    /// over it.
    #[instrument(level = "debug", skip(self, added, cfgs, run))]
    pub(super) fn end_context(
        &mut self,
        added: &ContextCrates,
        after: &str,
        cfgs: &LocalCfgs,
        run: Option<&mut FlipRun>,
    ) {
        if let Some(run) = run {
            let only_here = self.declared_only_when_flipped(cfgs, &run.ordinary);
            run.only.extend(only_here);
        }
        {
            let _finish = crate::Segment::begin("finish_context");
            self.finish_context(added, after);
        }
        self.report_slots(after);
    }

    /// Say how many module ids, block ids and cells exist, so that what grows with the contexts is named: module and
    /// block ids are never deleted, and the cells are.
    #[instrument(level = "debug", skip(self))]
    pub(super) fn report_slots(&self, when: &str) {
        let _slots = crate::Segment::begin("report_slots");
        let slots = <dyn salsa::Database>::struct_slots(
            &self.db,
            &["ModuleIdLt", "BlockIdLt", "ModuleCellLt", "DiagnosticsCellLt"],
        );
        let count = |name: &str| {
            slots.iter().find(|(found, _)| *found == name).map_or(0, |(_, count)| *count)
        };
        tracing::info!(
            target: "cordial::slots",
            when,
            module_ids = count("ModuleIdLt"),
            block_ids = count("BlockIdLt"),
            module_cells = count("ModuleCellLt"),
            diagnostics_cells = count("DiagnosticsCellLt"),
            "what the module and block ids and the cells hold"
        );
    }

    /// The contexts are all done: the ordinary cfg is written back, what the flips changed and what only they had
    /// is forgotten, and the maps are made again for what is verified next.
    #[instrument(level = "debug", skip(self, cfgs, run))]
    pub(super) fn finish_flips(&mut self, cfgs: &LocalCfgs, run: FlipRun) {
        let _finish = crate::Segment::begin("finish_flips");
        self.restore_cfgs(cfgs);
        let only: Vec<salsa::Id> = run.only.into_iter().collect();
        self.forget_flipped(&only);
        self.forget_changed_since(run.start);
        self.settle_flipped_crates(cfgs);
        self.say_the_crates("restored", None);
        self.report_slots("flips finished");
    }

    /// The flipped contexts are all done: the maps first, so that the modules the flips added are deleted and a
    /// block's module that is gone can be told; what is not declared is forgotten, which starts a revision; and
    /// the maps again in it, so that what is verified next reads only what has been made in the revision it is
    /// read in.
    pub(super) fn settle_flipped_crates(&mut self, cfgs: &LocalCfgs) {
        let _settle = crate::Segment::begin("settle_flipped");
        self.settle_ordinary(cfgs);
        self.forget_undeclared_roots();
        self.settle_ordinary(cfgs);
    }

    /// Where a block is, as text: the file whose code it is in, or that it is in what a macro made.
    #[instrument(level = "trace", skip(self))]
    fn block_owner(&self, block: ra_ap_hir_def::BlockId) -> String {
        match block.ast_id(&self.db).file_id.file_id() {
            Some(file) => self.vfs.file_path(file.file_id(&self.db)).to_string(),
            None => MACRO_OWNER.to_owned(),
        }
    }

    /// The owners of `blocks` with how many each has, most first.
    fn owners_top(blocks: &[&str]) -> String {
        let mut counted: std::collections::BTreeMap<&str, u64> = std::collections::BTreeMap::new();
        for owner in blocks {
            *counted.entry(owner).or_default() += 1;
        }
        let mut sorted: Vec<(&str, u64)> = counted.into_iter().collect();
        sorted.sort_by_key(|(owner, count)| (std::cmp::Reverse(*count), *owner));
        sorted.iter().take(5).map(|(owner, count)| format!("{owner}={count}")).collect::<Vec<_>>().join(" ")
    }

    /// The database's revision now, which a context's flips are measured from.
    pub(super) fn revision(&self) -> salsa::Revision {
        salsa::plumbing::current_revision(&self.db)
    }

    /// What the forget that has just ended dropped, by query, most first.
    fn forgotten_top() -> String {
        salsa::last_forgotten()
            .into_iter()
            .take(6)
            .map(|(name, count)| format!("{name}={count}"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Forget what a flipped context changed: the value of a query that is not what it was before the flip is
    /// the flipped state's, and is not verified or saved. A value that did not change is valid with the ordinary
    /// cfg back and stays. What made tracked structs stays too, as it runs again as any query does.
    #[instrument(level = "debug", skip(self))]
    pub(super) fn forget_changed_since(&mut self, revision: salsa::Revision) {
        let _forget = crate::Segment::begin("forget_changed");
        let started = std::time::Instant::now();
        let before = self.revision();
        let forgotten = salsa::Database::forget_memos_changed_after(&mut self.db, revision);
        tracing::info!(
            target: "cordial::eviction",
            forgotten,
            revision_before = ?before,
            revision_after = ?self.revision(),
            top = %Self::forgotten_top(),
            micros = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX),
            "what a flipped context changed is forgotten"
        );
    }

    /// Forget the saved queries that are keyed by an impl, a trait, an enum or a block that the definition map no
    /// longer has: they are the flipped state's, and verifying one runs it against a module that is gone. Said for
    /// each kind, how many are held and how many are not declared.
    #[instrument(level = "debug", skip(self))]
    pub(super) fn forget_undeclared_roots(&mut self) {
        use ra_ap_hir_def::declaration::{
            block_declaration, enum_declaration, impl_declaration, trait_declaration,
        };
        use ra_ap_hir_def::{BlockId, EnumId, ImplId, TraitId};
        use salsa::plumbing::FromId as _;
        let _forget = crate::Segment::begin("forget_undeclared");
        let started = std::time::Instant::now();
        let held = |db: &mut ra_ap_ide_db::RootDatabase, name: &str| {
            salsa::Database::memo_keys(db, &[name])
        };
        let impls = held(&mut self.db, "ImplItems::of_");
        let traits = held(&mut self.db, "TraitItems::query_with_diagnostics_");
        let enums = held(&mut self.db, "EnumVariants::of_");
        let blocks = held(&mut self.db, "block_def_map");
        let db = &self.db;
        let (dead_impls, dead_traits, dead_enums, dead_blocks) = attach_db(db, || {
            let dead = |keys: &[salsa::Id], declared: &dyn Fn(salsa::Id) -> bool| -> Vec<salsa::Id> {
                keys.iter().copied().filter(|key| !declared(*key)).collect()
            };
            (
                dead(&impls, &|key| impl_declaration(db, ImplId::from_id(key)).is_declared()),
                dead(&traits, &|key| trait_declaration(db, TraitId::from_id(key)).is_declared()),
                dead(&enums, &|key| enum_declaration(db, EnumId::from_id(key)).is_declared()),
                dead(&blocks, &|key| block_declaration(db, BlockId::from_id(key)).is_declared()),
            )
        });
        let mut all = Vec::new();
        for dead in [&dead_impls, &dead_traits, &dead_enums, &dead_blocks] {
            all.extend(dead.iter().copied());
        }
        // Why each undeclared block is not declared, before the forget takes what says so: gone (and how), or in a
        // module the map does not have.
        let reasons = {
            let db = &self.db;
            attach_db(db, || {
                let mut counted: std::collections::BTreeMap<String, u64> = std::collections::BTreeMap::new();
                for key in &dead_blocks {
                    if let ra_ap_hir_def::declaration::Declaration::Absent(absence) =
                        ra_ap_hir_def::declaration::block_declaration(db, BlockId::from_id(*key))
                    {
                        *counted.entry(format!("{absence:?}")).or_default() += 1;
                    }
                }
                counted.iter().map(|(reason, count)| format!("{reason}={count}")).collect::<Vec<_>>().join(" ")
            })
        };
        let before = self.revision();
        let forgotten = salsa::Database::forget_memos_for_inputs(&mut self.db, &all);
        tracing::info!(
            target: "cordial::eviction",
            revision_before = ?before,
            revision_after = ?self.revision(),
            blocks_absent_because = %reasons,
            impls = impls.len() as u64,
            impls_undeclared = dead_impls.len() as u64,
            traits = traits.len() as u64,
            traits_undeclared = dead_traits.len() as u64,
            enums = enums.len() as u64,
            enums_undeclared = dead_enums.len() as u64,
            blocks = blocks.len() as u64,
            blocks_undeclared = dead_blocks.len() as u64,
            forgotten,
            micros = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX),
            "saved queries keyed by what the definition map no longer declares are forgotten"
        );
    }

    /// Forget what was computed for `items`, which the ordinary cfg does not have.
    #[instrument(level = "debug", skip(self, items))]
    pub(super) fn forget_flipped(&mut self, items: &[salsa::Id]) {
        let _forget = crate::Segment::begin("forget_flipped");
        let before = self.revision();
        let forgotten = salsa::Database::forget_memos_for_inputs(&mut self.db, items);
        tracing::info!(
            target: "cordial::eviction",
            items = items.len() as u64,
            forgotten,
            revision_before = ?before,
            revision_after = ?self.revision(),
            top = %Self::forgotten_top(),
            "memos of items only a flipped context has are forgotten"
        );
    }

    fn module_hashes(&self, krate: InputCrate) -> HashMap<u64, u64> {
        attach_db(&self.db, || {
            module_fingerprints(&self.db, krate)
                .into_iter()
                .map(|module| (module.key, module.data))
                .collect()
        })
    }

    /// Say how many modules of the local crates the flipped cfg changed, added and removed against the ordinary
    /// ones: the definition work that was needed, whatever was done.
    #[instrument(level = "debug", skip(self, baseline))]
    pub(super) fn audit_flip(&self, baseline: &HashMap<InputCrate, HashMap<u64, u64>>, context: &SemanticContextKey) {
        let _audit = crate::Segment::begin("audit");
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

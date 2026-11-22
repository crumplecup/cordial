//! Exercise the actual RA map owner, projections, identities and saved cutoffs on tiny crates.
//! Memo counts describe storage paths, not retained bytes or process RSS.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Weak};

use ra_ap_cfg::CfgOptions;
use ra_ap_hir_def::nameres::{
    crate_declared_query_keys, crate_def_map, stable::def_map_fingerprint,
};
use ra_ap_hir_def::resolver::HasResolver;
use ra_ap_hir_def::{FunctionId, ModuleDefId};
use ra_ap_hir_expand::intern::Symbol;
use ra_ap_ide_db::RootDatabase;
use ra_ap_ide_db::base_db::{
    AbsPathBuf, Crate, CrateBuilderId, CrateGraphBuilder, CrateName, CrateOrigin,
    CrateWorkspaceData, DependencyBuilder, Env, FileChange, FileId, FileSet, SourceDatabase,
    SourceRoot, VfsPath,
};
use ra_ap_ide_db::span::Edition;
use salsa::plumbing::AsId;
use salsa::{Database, SpillKey, SpillStore};
use tracing_subscriber::layer::SubscriberExt;

static FIXTURE: Mutex<()> = Mutex::new(());
static EXTERNAL_READS: AtomicUsize = AtomicUsize::new(0);

#[salsa::tracked(returns(copy))]
fn external_items(db: &dyn SourceDatabase, krate: Crate) -> usize {
    EXTERNAL_READS.fetch_add(1, Ordering::SeqCst);
    crate_def_map(db, krate)
        .modules()
        .map(|(_, module)| module.scope.declarations().count())
        .sum()
}

#[salsa::tracked(persist, returns(copy))]
fn saved_external_items(db: &dyn SourceDatabase, krate: Crate) -> usize {
    external_items(db, krate)
}

#[salsa::tracked(persist, returns(copy))]
fn native_def_map_fingerprint(db: &dyn SourceDatabase, krate: Crate) -> u64 {
    let fingerprint = def_map_fingerprint(db, krate);
    assert_eq!(fingerprint.volatile, 0, "fixture must have a stable cutoff");
    fingerprint.hash
}

/// Use RA's real tracked module key with an immutable-field-only payload query.
#[salsa::tracked(persist, returns(ref))]
fn native_module_payload(
    db: &dyn SourceDatabase,
    module: ra_ap_hir_def::ModuleIdLt<'_>,
) -> Vec<u8> {
    vec![u8::from(module.containing_module_inside_def_map(db).is_some()); 4096]
}

#[derive(Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, salsa::SalsaValue)]
struct ItemPayload {
    bytes: Vec<u8>,
    #[serde(skip)]
    alive: Arc<()>,
}

/// An interned location key can own a payload without reading any input fields.
#[salsa::tracked(persist, returns(ref))]
fn native_item_payload(_db: &dyn SourceDatabase, function: FunctionId) -> ItemPayload {
    ItemPayload {
        bytes: vec![function.as_id().as_bits() as u8; 4096],
        alive: Arc::default(),
    }
}

struct Fixture {
    db: RootDatabase,
    graph: CrateGraphBuilder,
    main: CrateBuilderId,
    dependency: CrateBuilderId,
    krate: Crate,
    contexts: Vec<Crate>,
}

impl Fixture {
    fn new() -> Self {
        Self::build("pub mod nested { pub fn original() {} }", 0)
    }

    fn build(main_text: &str, contexts: usize) -> Self {
        let mut db = RootDatabase::default();
        let mut graph = CrateGraphBuilder::default();
        let mut files = FileSet::default();
        let mut change = FileChange::default();
        let mut roots = Vec::new();
        for id in 0..contexts + 2 {
            let text = if id == 1 {
                "pub struct Dependency; pub fn shared() {}"
            } else {
                main_text
            };
            let mut cfg = CfgOptions::default();
            if id >= 2 {
                cfg.insert_key_value(
                    Symbol::intern("variant"),
                    Symbol::intern(&(id - 2).to_string()),
                );
            }
            let id = u32::try_from(id).unwrap();
            let file = FileId::from_raw(id);
            files.insert(file, VfsPath::new_virtual_path(format!("/{id}/lib.rs")));
            change.change_file(file, Some(text.to_owned()));
            roots.push(graph.add_crate_root(
                file,
                Edition::CURRENT,
                None,
                None,
                cfg,
                None,
                Env::default(),
                CrateOrigin::Local {
                    repo: None,
                    name: None,
                },
                Vec::new(),
                false,
                triomphe::Arc::new(AbsPathBuf::assert("/".into())),
                triomphe::Arc::new(CrateWorkspaceData {
                    target: Err("fixture has no target".into()),
                    toolchain: None,
                }),
            ));
        }
        change.set_roots(vec![SourceRoot::new_local(files)]);
        change.set_crate_graph(graph.clone());
        let crates = change.apply(&mut db).unwrap();
        let krate = crates[&roots[0]];
        let contexts = roots[2..].iter().map(|id| crates[id]).collect();
        Self {
            db,
            graph,
            main: roots[0],
            dependency: roots[1],
            krate,
            contexts,
        }
    }

    fn restore(&mut self) {
        let files = self.db.files().dump();
        let crates = self.db.crates_map().dump();
        let saved = rmp_serde::to_vec(&<dyn Database>::as_serialize(&mut self.db)).unwrap();
        let mut restored = RootDatabase::empty();
        <dyn Database>::deserialize(
            &mut restored,
            &mut rmp_serde::Deserializer::new(saved.as_slice()),
        )
        .unwrap();
        restored.files().restore(files);
        restored.crates_map().restore(crates);
        // Match session restoration: verification may reach RA queries before a direct call.
        let source: &dyn SourceDatabase = &restored;
        let _ = SourceDatabase::zalsa_register_downcaster(source);
        <dyn Database>::init_view_casters(&restored);
        self.db = restored;
    }

    fn add_dependency(&mut self) {
        let crates: Vec<_> = self
            .graph
            .iter()
            .filter(|id| *id != self.dependency)
            .collect();
        for krate in crates {
            self.graph
                .add_dep(
                    krate,
                    DependencyBuilder::new(CrateName::new("unused").unwrap(), self.dependency),
                )
                .unwrap();
        }
        let crates = self.graph.clone().set_in_db(&mut self.db);
        assert_eq!(crates[&self.main], self.krate);
    }

    fn edit(&mut self, text: &str) {
        let mut change = FileChange::default();
        change.change_file(FileId::from_raw(0), Some(text.to_owned()));
        change.apply(&mut self.db);
    }

    fn modules(&self) -> Vec<ra_ap_hir_def::ModuleId> {
        crate_def_map(&self.db, self.krate)
            .modules()
            .map(|(id, _)| id)
            .collect()
    }

    fn dependencies(&self) -> usize {
        crate_def_map(&self.db, self.krate)
            .crate_root(&self.db)
            .resolver(&self.db)
            .extern_crates_in_scope()
            .count()
    }
}

#[derive(Default)]
struct Store(Mutex<HashMap<SpillKey, Vec<u8>>>);

impl SpillStore for Store {
    fn put(&self, key: SpillKey, bytes: &[u8]) {
        self.0.lock().unwrap().insert(key, bytes.to_vec());
    }

    fn get(&self, key: SpillKey) -> Option<Vec<u8>> {
        self.0.lock().unwrap().get(&key).cloned()
    }
}

struct ResetStore;

impl Drop for ResetStore {
    fn drop(&mut self) {
        salsa::set_spill_store(None);
    }
}

#[derive(Clone, Default)]
struct MapExecutions(Arc<AtomicUsize>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for MapExecutions {
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        _: &tracing::Id,
        _: tracing_subscriber::layer::Context<'_, S>,
    ) {
        if attrs.metadata().name() == "crate_def_map_query" {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
}

fn declared_functions(db: &RootDatabase, krate: Crate) -> Vec<FunctionId> {
    let mut functions: Vec<_> = crate_def_map(db, krate)
        .modules()
        .flat_map(|(_, module)| module.scope.declarations())
        .filter_map(|def| match def {
            ModuleDefId::FunctionId(id) => Some(id),
            _ => None,
        })
        .collect();
    functions.sort_unstable_by_key(|id| id.as_id().as_bits());
    functions
}

fn warm_items(db: &RootDatabase, functions: &[FunctionId]) -> Vec<Weak<()>> {
    functions
        .iter()
        .map(|&function| {
            ra_ap_hir_def::expr_store::body::Body::with_source_map(db, function.into());
            Arc::downgrade(&native_item_payload(db, function).alive)
        })
        .collect()
}

fn declared_context_keys(db: &mut RootDatabase, krate: Crate) -> Vec<salsa::Id> {
    let mut seeds = vec![krate.as_id()];
    seeds.extend(crate_declared_query_keys(db, krate));
    db.tracked_output_keys(&seeds)
}

fn held_item_values(db: &mut RootDatabase, functions: &[FunctionId]) -> u64 {
    let keys: Vec<_> = functions.iter().map(AsId::as_id).collect();
    let mut held = 0;
    db.values_in_memory_for_inputs(
        &keys,
        false,
        true,
        &["native_item_payload", "Body::with_source_map_"],
        &mut |_, count| held += count,
    );
    held
}

#[test]
fn declared_locations_retire_cfg_item_values_without_retiring_imported_work() {
    let _fixture = FIXTURE.lock().unwrap();
    let _reset = ResetStore;
    for count in [1, 4, 16] {
        let mut source = String::from(
            "extern crate unused; pub use unused::Dependency; use unused::shared;\n\
             macro_rules! make { () => { pub fn generated() {} } } make!();\n\
             pub mod nested { pub fn inside() {} }\n\
             pub fn always() {}\n",
        );
        for variant in 0..count {
            source.push_str(&format!(
                "#[cfg(variant = \"{variant}\")] pub fn selected() {{ let _ = {variant}; }}\n"
            ));
        }
        for restored in [false, true] {
            salsa::set_spill_store(Some(Arc::new(Store::default())));
            let executions = MapExecutions::default();
            let subscriber = tracing_subscriber::registry().with(executions.clone());
            tracing::subscriber::with_default(subscriber, || {
                let mut fixture = Fixture::build(&source, count);
                fixture.add_dependency();
                let dependency = ra_ap_ide_db::base_db::all_crates(&fixture.db)
                    .iter()
                    .copied()
                    .find(|krate| *krate != fixture.krate && !fixture.contexts.contains(krate))
                    .unwrap();
                let ordinary = declared_functions(&fixture.db, fixture.krate);
                assert_eq!(ordinary.len(), 3, "ordinary cfg excludes selected()");
                let functions: Vec<_> = fixture
                    .contexts
                    .iter()
                    .map(|&krate| declared_functions(&fixture.db, krate))
                    .collect();
                assert!(functions.iter().all(|items| items.len() == 4));
                // Shared payloads are first warmed while processing the first cfg copy.
                for items in &functions {
                    warm_items(&fixture.db, items);
                    warm_items(&fixture.db, &declared_functions(&fixture.db, dependency));
                }
                warm_items(&fixture.db, &ordinary);
                if restored {
                    fixture.restore();
                }
                let shared = declared_functions(&fixture.db, dependency);
                assert_eq!(shared.len(), 1);
                let shared_markers = warm_items(&fixture.db, &shared);
                let ordinary_markers = warm_items(&fixture.db, &ordinary);
                let markers: Vec<_> = functions
                    .iter()
                    .map(|items| warm_items(&fixture.db, items))
                    .collect();
                let map_count = count + 2;
                assert_eq!(executions.0.load(Ordering::SeqCst), map_count);
                for (index, &krate) in fixture.contexts.iter().enumerate() {
                    assert_eq!(declared_functions(&fixture.db, krate), functions[index]);
                    assert!(
                        crate_def_map(&fixture.db, krate)
                            .modules()
                            .any(|(_, module)| {
                                module
                                    .scope
                                    .values()
                                    .any(|(_, item)| item.def == ModuleDefId::FunctionId(shared[0]))
                            }),
                        "the fixture must actually import shared()"
                    );
                    let keys = declared_context_keys(&mut fixture.db, krate);
                    assert!(functions[index].iter().all(|id| keys.contains(&id.as_id())));
                    assert!(
                        ordinary
                            .iter()
                            .chain(&shared)
                            .all(|id| !keys.contains(&id.as_id()))
                    );
                    for _ in 0..2 {
                        fixture.db.values_in_memory_for_inputs(
                            &keys,
                            true,
                            false,
                            &[],
                            &mut |_, _| {},
                        );
                        fixture.db.spill_persisted_for_inputs(&keys);
                        assert_eq!(held_item_values(&mut fixture.db, &functions[index]), 0);
                    }
                    for (other, markers) in markers.iter().enumerate() {
                        assert!(
                            markers
                                .iter()
                                .all(|marker| { (marker.strong_count() > 0) == (other > index) }),
                            "only completed cfg copies should release original payloads"
                        );
                    }
                    assert!(
                        ordinary_markers
                            .iter()
                            .chain(&shared_markers)
                            .all(|marker| marker.strong_count() > 0)
                    );
                    assert_eq!(
                        held_item_values(&mut fixture.db, &ordinary),
                        2 * ordinary.len() as u64
                    );
                    assert_eq!(held_item_values(&mut fixture.db, &shared), 2);
                }
                for (index, &krate) in fixture.contexts.iter().enumerate() {
                    assert_eq!(declared_functions(&fixture.db, krate), functions[index]);
                    for &function in &functions[index] {
                        assert_eq!(
                            native_item_payload(&fixture.db, function).bytes,
                            vec![function.as_id().as_bits() as u8; 4096]
                        );
                        ra_ap_hir_def::expr_store::body::Body::with_source_map(
                            &fixture.db,
                            function.into(),
                        );
                    }
                    assert_eq!(held_item_values(&mut fixture.db, &functions[index]), 8);
                }
                assert_eq!(
                    executions.0.load(Ordering::SeqCst),
                    map_count,
                    "re-entry decodes maps without collecting them"
                );
            });
        }
    }
}

#[test]
fn declared_location_selection_follows_the_current_map_after_an_edit() {
    let _fixture = FIXTURE.lock().unwrap();
    let _reset = ResetStore;
    for restored in [false, true] {
        salsa::set_spill_store(Some(Arc::new(Store::default())));
        let mut fixture = Fixture::build(
            "extern crate unused; use unused::shared; pub fn original() {}",
            1,
        );
        fixture.add_dependency();
        let context = fixture.contexts[0];
        let previous = declared_functions(&fixture.db, context);
        assert_eq!(previous.len(), 1);
        warm_items(&fixture.db, &previous);
        if restored {
            fixture.restore();
        }
        let ordinary = declared_functions(&fixture.db, fixture.krate);
        let ordinary_markers = warm_items(&fixture.db, &ordinary);
        let mut change = FileChange::default();
        change.change_file(
            FileId::from_raw(2),
            Some("extern crate unused; use unused::shared; pub fn replacement() {}".into()),
        );
        change.apply(&mut fixture.db);
        let current = declared_functions(&fixture.db, context);
        assert_eq!(current.len(), 1);
        assert_ne!(current, previous);
        let markers = warm_items(&fixture.db, &current);
        let keys = declared_context_keys(&mut fixture.db, context);
        assert!(current.iter().all(|id| keys.contains(&id.as_id())));
        assert!(
            previous
                .iter()
                .chain(&ordinary)
                .all(|id| !keys.contains(&id.as_id()))
        );
        fixture
            .db
            .values_in_memory_for_inputs(&keys, true, false, &[], &mut |_, _| {});
        fixture.db.spill_persisted_for_inputs(&keys);
        assert_eq!(held_item_values(&mut fixture.db, &current), 0);
        assert!(markers.iter().all(|marker| marker.strong_count() == 0));
        assert!(
            ordinary_markers
                .iter()
                .all(|marker| marker.strong_count() > 0)
        );
        assert_eq!(declared_functions(&fixture.db, context), current);
        warm_items(&fixture.db, &current);
        assert_eq!(held_item_values(&mut fixture.db, &current), 2);
    }
}

#[test]
fn an_unused_dependency_backdates_external_reads_and_public_edits_reach_them() {
    let _fixture = FIXTURE.lock().unwrap();
    let _reset = ResetStore;
    for restored in [false, true] {
        for spilled in [false, true] {
            salsa::set_spill_store(Some(Arc::new(Store::default())));
            let mut fixture = Fixture::new();
            let items = external_items(&fixture.db, fixture.krate);
            assert_eq!(items, 2);
            let modules = fixture.modules();
            let fingerprint = def_map_fingerprint(&fixture.db, fixture.krate);
            assert_eq!(fixture.dependencies(), 0);
            if restored {
                fixture.restore();
                // Warm the non-persisted projections before testing incremental backdating.
                fixture.dependencies();
                assert_eq!(external_items(&fixture.db, fixture.krate), items);
            }
            if spilled {
                fixture.db.spill_persisted();
            }
            let before = EXTERNAL_READS.load(Ordering::SeqCst);
            fixture.add_dependency();
            assert_eq!(fixture.dependencies(), 1, "the local prelude changed");
            assert_ne!(def_map_fingerprint(&fixture.db, fixture.krate), fingerprint);
            assert_eq!(external_items(&fixture.db, fixture.krate), items);
            assert_eq!(
                EXTERNAL_READS.load(Ordering::SeqCst),
                before,
                "restored={restored}, spilled={spilled}"
            );
            assert_eq!(fixture.modules(), modules);

            fixture.edit("pub mod nested { pub fn original() {} pub fn added() {} }");
            assert_eq!(external_items(&fixture.db, fixture.krate), items + 1);
            assert_eq!(EXTERNAL_READS.load(Ordering::SeqCst), before + 1);
            assert_eq!(fixture.modules(), modules);
        }
    }
}

#[test]
fn spilled_maps_reenter_without_collection_and_keep_module_identity() {
    let _fixture = FIXTURE.lock().unwrap();
    let _reset = ResetStore;
    salsa::set_spill_store(Some(Arc::new(Store::default())));
    let executions = MapExecutions::default();
    let subscriber = tracing_subscriber::registry().with(executions.clone());
    tracing::subscriber::with_default(subscriber, || {
        let mut fixture = Fixture::new();
        let modules = fixture.modules();
        let fingerprint = def_map_fingerprint(&fixture.db, fixture.krate);
        ra_ap_hir_def::lang_item::lang_items(&fixture.db, fixture.krate);
        let mut projections = HashMap::new();
        fixture.db.values_in_memory(
            false,
            false,
            &["external_def_map", "local_def_map"],
            &mut |name, count| {
                projections.insert(name, count);
            },
        );
        assert_eq!(projections.get("external_def_map"), Some(&1));
        assert_eq!(projections.get("local_def_map"), Some(&1));
        assert_eq!(executions.0.load(Ordering::SeqCst), 1);
        for round in 0..2 {
            assert!(fixture.db.drop_regenerable_values() > 0);
            let report = fixture.db.spill_persisted();
            if round == 0 {
                let owner = report
                    .by_query
                    .iter()
                    .find(|row| row.query == "crate_local_def_map")
                    .unwrap();
                assert_eq!(owner.values, 1);
                assert!(owner.bytes > 0);
            } else {
                assert!(
                    report.dropped_again > 0,
                    "decoded owners can be released again"
                );
            }
            assert!(
                report
                    .by_query
                    .iter()
                    .all(|row| row.query != "external_def_map" && row.query != "local_def_map")
            );
            let mut held = 0;
            fixture.db.values_in_memory(
                false,
                true,
                &["crate_local_def_map", "external_def_map", "local_def_map"],
                &mut |_, count| held += count,
            );
            assert_eq!(
                held, 0,
                "owner and projection memos have no resident values"
            );
            assert_eq!(fixture.modules(), modules);
            assert_eq!(def_map_fingerprint(&fixture.db, fixture.krate), fingerprint);
            assert_eq!(
                executions.0.load(Ordering::SeqCst),
                1,
                "decoded, not collected"
            );
        }
    });
}

#[test]
fn retiring_one_crate_preserves_dependency_maps_and_reenters_without_collection() {
    let _fixture = FIXTURE.lock().unwrap();
    let _reset = ResetStore;
    for restored in [false, true] {
        salsa::set_spill_store(Some(Arc::new(Store::default())));
        let executions = MapExecutions::default();
        let subscriber = tracing_subscriber::registry().with(executions.clone());
        tracing::subscriber::with_default(subscriber, || {
            let mut fixture = Fixture::new();
            fixture.add_dependency();
            let dependency = ra_ap_ide_db::base_db::all_crates(&fixture.db)
                .iter()
                .copied()
                .find(|krate| *krate != fixture.krate)
                .unwrap();
            external_items(&fixture.db, dependency);
            ra_ap_hir_def::lang_item::lang_items(&fixture.db, dependency);
            let modules = fixture.modules();
            let fingerprint = def_map_fingerprint(&fixture.db, fixture.krate);
            ra_ap_hir_def::lang_item::lang_items(&fixture.db, fixture.krate);
            let dependency_modules: Vec<_> = crate_def_map(&fixture.db, dependency)
                .modules()
                .map(|(id, _)| id)
                .collect();
            for &module in modules.iter().chain(&dependency_modules) {
                native_module_payload(&fixture.db, module);
            }
            if restored {
                fixture.restore();
                for krate in [fixture.krate, dependency] {
                    external_items(&fixture.db, krate);
                    ra_ap_hir_def::lang_item::lang_items(&fixture.db, krate);
                }
                let selected = fixture.db.tracked_output_keys(&[fixture.krate.as_id()]);
                assert!(
                    modules
                        .iter()
                        .all(|module| selected.contains(&module.as_id()))
                );
                // Persisted payloads restore as encoded bytes. Warm both sides so the
                // retirement check below compares decoded resident values.
                for &module in modules.iter().chain(&dependency_modules) {
                    native_module_payload(&fixture.db, module);
                }
            }
            let held_maps = |db: &mut RootDatabase, krate: Crate| {
                let mut held = 0;
                db.values_in_memory_for_inputs(
                    &[krate.as_id()],
                    false,
                    true,
                    &["crate_local_def_map", "external_def_map", "local_def_map"],
                    &mut |_, count| held += count,
                );
                held
            };
            // Restored lang_items decodes its saved value without reading the non-persisted
            // local projection. Cold reads warm all three maps; restored reads warm two.
            let shared_held = if restored { 2 } else { 3 };
            assert_eq!(held_maps(&mut fixture.db, fixture.krate), shared_held);
            assert_eq!(held_maps(&mut fixture.db, dependency), shared_held);
            assert_eq!(executions.0.load(Ordering::SeqCst), 2);
            for _ in 0..2 {
                let inputs = fixture.db.tracked_output_keys(&[fixture.krate.as_id()]);
                for module in &modules {
                    assert!(inputs.contains(&module.as_id()));
                }
                for module in &dependency_modules {
                    assert!(!inputs.contains(&module.as_id()));
                }
                fixture
                    .db
                    .values_in_memory_for_inputs(&inputs, true, false, &[], &mut |_, _| {});
                let report = fixture.db.spill_persisted_for_inputs(&inputs);
                assert!(report.written + report.dropped_again > 0);
                assert_eq!(held_maps(&mut fixture.db, fixture.krate), 0);
                assert_eq!(held_maps(&mut fixture.db, dependency), shared_held);
                for (expected, modules) in [(0, &modules), (1, &dependency_modules)] {
                    for module in modules {
                        let mut held = 0;
                        fixture.db.values_in_memory_for_inputs(
                            &[module.as_id()],
                            false,
                            true,
                            &["native_module_payload"],
                            &mut |_, count| held += count,
                        );
                        assert_eq!(held, expected);
                    }
                }
                let reads = EXTERNAL_READS.load(Ordering::SeqCst);
                external_items(&fixture.db, dependency);
                assert_eq!(EXTERNAL_READS.load(Ordering::SeqCst), reads);
                assert_eq!(fixture.modules(), modules);
                assert_eq!(def_map_fingerprint(&fixture.db, fixture.krate), fingerprint);
                ra_ap_hir_def::lang_item::lang_items(&fixture.db, fixture.krate);
                for &module in &modules {
                    let expected = u8::from(
                        module
                            .containing_module_inside_def_map(&fixture.db)
                            .is_some(),
                    );
                    assert_eq!(
                        native_module_payload(&fixture.db, module),
                        &vec![expected; 4096]
                    );
                }
                assert_eq!(executions.0.load(Ordering::SeqCst), 2);
            }
        });
    }
}

#[test]
fn restored_cutoffs_reuse_body_edits_and_invalidate_public_edits() {
    let _fixture = FIXTURE.lock().unwrap();
    let mut fixture = Fixture::new();
    assert!(<dyn Database>::register_cutoff(
        &fixture.db,
        "crate_local_def_map",
        "native_def_map_fingerprint"
    ));
    let items = saved_external_items(&fixture.db, fixture.krate);
    let fingerprint = native_def_map_fingerprint(&fixture.db, fixture.krate);
    let modules = fixture.modules();
    fixture.restore();
    assert!(<dyn Database>::register_cutoff(
        &fixture.db,
        "crate_local_def_map",
        "native_def_map_fingerprint"
    ));
    assert_eq!(
        native_def_map_fingerprint(&fixture.db, fixture.krate),
        fingerprint
    );
    let before = EXTERNAL_READS.load(Ordering::SeqCst);
    fixture.edit("pub mod nested { pub fn original() { let _value = 3; } }");
    assert_eq!(saved_external_items(&fixture.db, fixture.krate), items);
    assert_eq!(EXTERNAL_READS.load(Ordering::SeqCst), before);
    assert_eq!(
        native_def_map_fingerprint(&fixture.db, fixture.krate),
        fingerprint
    );
    assert_eq!(
        fixture.modules(),
        modules,
        "collection preserves restored module identities"
    );

    fixture.edit("pub mod nested { pub fn original() {} pub fn added() {} }");
    assert_eq!(saved_external_items(&fixture.db, fixture.krate), items + 1);
    assert_eq!(EXTERNAL_READS.load(Ordering::SeqCst), before + 1);
    assert_ne!(
        native_def_map_fingerprint(&fixture.db, fixture.krate),
        fingerprint
    );
    assert_eq!(fixture.modules(), modules);
}

//! Exercise the actual RA map owner, projections, identities and saved cutoffs on tiny crates.
//! Memo counts describe storage paths, not retained bytes or process RSS.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Weak};

use ra_ap_cfg::CfgOptions;
use ra_ap_hir_def::expr_store::body::Body;
use ra_ap_hir_def::item_tree::{BlockItemTreeKey, FileItemTreeKey, file_item_tree};
use ra_ap_hir_def::nameres::{
    ModuleOrigin, block_def_map, crate_declared_query_keys, crate_def_map,
    stable::def_map_fingerprint,
};
use ra_ap_hir_def::resolver::HasResolver;
use ra_ap_hir_def::signatures::VariantFields;
use ra_ap_hir_def::{
    AdtId, AssocItemId, BlockId, DefWithBodyId, EnumVariantId, FunctionId, MacroId, ModuleDefId,
};
use ra_ap_hir_expand::HirFileId;
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

/// Witness release for payloads keyed directly by a local macro's interned identity.
#[salsa::tracked(persist, returns(ref))]
fn native_macro_payload(_db: &dyn SourceDatabase, mac: MacroId) -> ItemPayload {
    ItemPayload {
        bytes: vec![mac.as_id().as_bits() as u8; 4096],
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
        Self::build_with_dependency(
            main_text,
            contexts,
            "pub struct Dependency; pub fn shared() {}",
        )
    }

    fn build_with_dependency(main_text: &str, contexts: usize, dependency_text: &str) -> Self {
        let mut db = RootDatabase::default();
        let mut graph = CrateGraphBuilder::default();
        let mut files = FileSet::default();
        let mut change = FileChange::default();
        let mut roots = Vec::new();
        for id in 0..contexts + 2 {
            let text = if id == 1 { dependency_text } else { main_text };
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

fn declared_macros(db: &RootDatabase, krate: Crate) -> Vec<MacroId> {
    let mut macros: Vec<_> = crate_def_map(db, krate)
        .macro_def_to_macro_id
        .values()
        .copied()
        .collect();
    macros.sort_unstable_by_key(|id| id.as_id().as_bits());
    macros
}

fn warm_macros(db: &RootDatabase, macros: &[MacroId]) -> Vec<Weak<()>> {
    macros
        .iter()
        .map(|&mac| {
            mac.definition(db);
            ra_ap_hir_def::attrs::AttrFlags::query(db, mac.into());
            if let Some(key) = mac.declarative_expander_key(db) {
                key.expander(db);
            }
            Arc::downgrade(&native_macro_payload(db, mac).alive)
        })
        .collect()
}

fn expander_keys(db: &RootDatabase, macros: &[MacroId]) -> Vec<salsa::Id> {
    macros
        .iter()
        .filter_map(|&mac| mac.declarative_expander_key(db).map(|key| key.as_id()))
        .collect()
}

fn held_expanders(db: &mut RootDatabase, macros: &[MacroId]) -> u64 {
    let keys = expander_keys(db, macros);
    let mut held = 0;
    db.values_in_memory_for_inputs(&keys, false, true, &[], &mut |_, count| held += count);
    held
}

#[derive(Clone, Default)]
struct MacroExecutions(Arc<AtomicUsize>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for MacroExecutions {
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        _: &tracing::Id,
        _: tracing_subscriber::layer::Context<'_, S>,
    ) {
        if attrs.metadata().name() == "decl_macro_expander_query" {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
}

fn held_macro_values(db: &mut RootDatabase, macros: &[MacroId]) -> u64 {
    let keys: Vec<_> = macros.iter().map(AsId::as_id).collect();
    let mut held = 0;
    db.values_in_memory_for_inputs(
        &keys,
        false,
        true,
        &[
            "native_macro_payload",
            "MacroId::definition_",
            "AttrFlags::query_",
        ],
        &mut |_, count| held += count,
    );
    held
}

#[derive(Debug, PartialEq, Eq)]
struct LocalBodies {
    bodies: Vec<DefWithBodyId>,
    blocks: Vec<BlockId>,
    macros: Vec<MacroId>,
}

impl LocalBodies {
    fn collect(db: &RootDatabase, krate: Crate) -> Self {
        let mut facts = Self {
            bodies: Vec::new(),
            blocks: Vec::new(),
            macros: Vec::new(),
        };
        facts.collect_map(db, crate_def_map(db, krate));
        let mut next = 0;
        while next < facts.bodies.len() {
            let body = &Body::with_source_map(db, facts.bodies[next]).0;
            for (block, map) in body.blocks(db) {
                if !facts.blocks.contains(&block) {
                    facts.blocks.push(block);
                    facts.collect_map(db, map);
                }
            }
            next += 1;
        }
        facts.bodies.sort_unstable_by_key(|id| id.as_id().as_bits());
        facts.blocks.sort_unstable_by_key(|id| id.as_id().as_bits());
        facts.macros.sort_unstable_by_key(|id| id.as_id().as_bits());
        facts
    }

    fn collect_map(&mut self, db: &RootDatabase, map: &ra_ap_hir_def::nameres::DefMap) {
        for &mac in map.macro_def_to_macro_id.values() {
            if !self.macros.contains(&mac) {
                self.macros.push(mac);
            }
        }
        let mut bodies = Vec::new();
        for (_, module) in map.modules() {
            for def in module.scope.declarations() {
                match def {
                    ModuleDefId::FunctionId(id) => bodies.push(id.into()),
                    ModuleDefId::ConstId(id) => bodies.push(id.into()),
                    ModuleDefId::StaticId(id) => bodies.push(id.into()),
                    ModuleDefId::AdtId(AdtId::EnumId(id)) => bodies.extend(
                        id.enum_variants(db)
                            .variants
                            .values()
                            .map(|(id, _)| DefWithBodyId::VariantId(*id)),
                    ),
                    ModuleDefId::TraitId(id) => {
                        bodies.extend(id.trait_items(db).items.iter().filter_map(
                            |(_, id)| match id {
                                AssocItemId::FunctionId(id) => Some(DefWithBodyId::FunctionId(*id)),
                                AssocItemId::ConstId(id) => Some(DefWithBodyId::ConstId(*id)),
                                _ => None,
                            },
                        ));
                    }
                    _ => {}
                }
            }
            for id in module.scope.impls() {
                bodies.extend(
                    id.impl_items(db)
                        .items
                        .iter()
                        .filter_map(|(_, id)| match id {
                            AssocItemId::FunctionId(id) => Some(DefWithBodyId::FunctionId(*id)),
                            AssocItemId::ConstId(id) => Some(DefWithBodyId::ConstId(*id)),
                            _ => None,
                        }),
                );
            }
        }
        for body in bodies {
            if !self.bodies.contains(&body) {
                self.bodies.push(body);
            }
        }
    }

    fn warm(&self, db: &RootDatabase) -> Vec<triomphe::Arc<Body>> {
        self.bodies
            .iter()
            .map(|&id| {
                Body::of(db, id);
                Body::with_source_map(db, id).0.clone()
            })
            .collect()
    }

    fn held(&self, db: &mut RootDatabase) -> u64 {
        let keys: Vec<_> = self.bodies.iter().map(AsId::as_id).collect();
        let mut held = 0;
        db.values_in_memory_for_inputs(
            &keys,
            false,
            true,
            &["Body::of_", "Body::with_source_map_"],
            &mut |_, count| held += count,
        );
        held
    }
}

#[derive(Clone, Default)]
struct LocalExecutions {
    blocks: Arc<AtomicUsize>,
    bodies: Arc<AtomicUsize>,
}

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for LocalExecutions {
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        _: &tracing::Id,
        _: tracing_subscriber::layer::Context<'_, S>,
    ) {
        match attrs.metadata().name() {
            "block_def_map_query" => {
                self.blocks.fetch_add(1, Ordering::SeqCst);
            }
            "body_with_source_map_query" => {
                self.bodies.fetch_add(1, Ordering::SeqCst);
            }
            _ => {}
        }
    }
}

fn associated_functions(db: &RootDatabase, krate: Crate) -> Vec<FunctionId> {
    let mut functions = Vec::new();
    for (_, module) in crate_def_map(db, krate).modules() {
        let traits = module.scope.declarations().filter_map(|def| match def {
            ModuleDefId::TraitId(id) => Some(id),
            _ => None,
        });
        for items in traits
            .map(|id| &id.trait_items(db).items)
            .chain(module.scope.impls().map(|id| &id.impl_items(db).items))
        {
            functions.extend(items.iter().filter_map(|(_, id)| match id {
                AssocItemId::FunctionId(id) => Some(*id),
                _ => None,
            }));
        }
    }
    functions.sort_unstable_by_key(|id| id.as_id().as_bits());
    functions
}

fn associated_query_keys(db: &RootDatabase, krate: Crate) -> Vec<salsa::Id> {
    let mut keys = Vec::new();
    for (_, module) in crate_def_map(db, krate).modules() {
        for def in module.scope.declarations() {
            if let ModuleDefId::TraitId(id) = def {
                let members = id.trait_items(db);
                keys.extend(members.items.iter().map(|(_, id)| id.as_id()));
                keys.extend(members.macro_calls().map(|(_, id)| id.as_id()));
            }
        }
        for id in module.scope.impls() {
            let members = id.impl_items(db);
            keys.extend(members.items.iter().map(|(_, id)| id.as_id()));
            keys.extend(members.macro_calls().map(|(_, id)| id.as_id()));
        }
    }
    keys
}

#[derive(Clone, Default)]
struct MemberExecutions(Arc<AtomicUsize>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for MemberExecutions {
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        _: &tracing::Id,
        _: tracing_subscriber::layer::Context<'_, S>,
    ) {
        if matches!(
            attrs.metadata().name(),
            "trait_items_with_diagnostics_query" | "impl_items_with_diagnostics_query"
        ) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
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

fn declared_variants(db: &RootDatabase, krate: Crate) -> (Vec<EnumVariantId>, usize) {
    let mut variants = Vec::new();
    let mut inactive = 0;
    for (_, module) in crate_def_map(db, krate).modules() {
        for def in module.scope.declarations() {
            if let ModuleDefId::AdtId(AdtId::EnumId(id)) = def {
                let (members, diagnostics) = id.enum_variants_with_diagnostics(db);
                variants.extend(members.variants.values().map(|(id, _)| *id));
                inactive += diagnostics.len();
            }
        }
    }
    variants.sort_unstable_by_key(|id| id.as_id().as_bits());
    (variants, inactive)
}

#[derive(Clone, Default)]
struct VariantExecutions(Arc<AtomicUsize>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for VariantExecutions {
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        _: &tracing::Id,
        _: tracing_subscriber::layer::Context<'_, S>,
    ) {
        if attrs.metadata().name() == "enum_variants_query" {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
}

struct VariantValues {
    body: triomphe::Arc<Body>,
    fields: triomphe::Arc<VariantFields>,
}

impl VariantValues {
    fn assert_held(&self, expected: bool) {
        // Triomphe has no weak pointers. Exclude this test's one retained witness;
        // count == 1 proves that no database owner still holds the original value.
        assert_eq!(triomphe::Arc::strong_count(&self.body) > 1, expected);
        assert_eq!(triomphe::Arc::strong_count(&self.fields) > 1, expected);
    }
}

fn warm_variants(db: &RootDatabase, variants: &[EnumVariantId]) -> Vec<VariantValues> {
    variants
        .iter()
        .map(|&variant| {
            let body = Body::with_source_map(db, variant.into());
            variant.fields(db);
            let fields = VariantFields::with_source_map(db, variant.into());
            VariantValues {
                body: triomphe::Arc::clone(&body.0),
                fields: triomphe::Arc::clone(&fields.0),
            }
        })
        .collect()
}

fn held_variant_values(db: &mut RootDatabase, variants: &[EnumVariantId]) -> u64 {
    let keys: Vec<_> = variants.iter().map(AsId::as_id).collect();
    let mut held = 0;
    db.values_in_memory_for_inputs(
        &keys,
        false,
        true,
        &[
            "Body::with_source_map_",
            "VariantFields::of_",
            "VariantFields::with_source_map_",
        ],
        &mut |_, count| held += count,
    );
    held
}

#[test]
fn enum_variants_retire_real_bodies_and_fields_without_collecting_lists() {
    let _fixture = FIXTURE.lock().unwrap();
    let _reset = ResetStore;
    for count in [1, 4, 16] {
        let mut source = String::from(
            "extern crate unused; pub use unused::Shared;\n\
             macro_rules! make { () => { pub enum Generated { Made = 2 } } } make!();\n\
             pub enum Shapes { Unit, Tuple(u8), Record { value: u8 } }\n\
             pub enum Local { Default = 1,\n",
        );
        for variant in 0..count {
            source.push_str(&format!(
                "#[cfg(variant = \"{variant}\")] Selected{variant} = {},\n",
                variant + 2
            ));
        }
        source.push_str("}\n");
        for restored in [false, true] {
            salsa::set_spill_store(Some(Arc::new(Store::default())));
            let executions = VariantExecutions::default();
            let subscriber = tracing_subscriber::registry().with(executions.clone());
            tracing::subscriber::with_default(subscriber, || {
                let mut fixture = Fixture::build_with_dependency(
                    &source,
                    count,
                    "#[repr(u8)] pub enum Shared { Unit = 3, Tuple(u8) = 4 }",
                );
                fixture.add_dependency();
                let dependency = ra_ap_ide_db::base_db::all_crates(&fixture.db)
                    .iter()
                    .copied()
                    .find(|krate| *krate != fixture.krate && !fixture.contexts.contains(krate))
                    .unwrap();
                let (ordinary, inactive) = declared_variants(&fixture.db, fixture.krate);
                assert_eq!(ordinary.len(), 5);
                assert_eq!(inactive, count);
                let variants: Vec<_> = fixture
                    .contexts
                    .iter()
                    .map(|&krate| {
                        let (variants, inactive) = declared_variants(&fixture.db, krate);
                        assert_eq!(variants.len(), 6);
                        assert_eq!(inactive, count - 1);
                        variants
                    })
                    .collect();
                let (shared, _) = declared_variants(&fixture.db, dependency);
                assert_eq!(shared.len(), 2);
                for items in &variants {
                    warm_variants(&fixture.db, items);
                    warm_variants(&fixture.db, &shared);
                }
                warm_variants(&fixture.db, &ordinary);
                let collected = executions.0.load(Ordering::SeqCst);
                assert!(collected > 0);
                if restored {
                    fixture.restore();
                }
                let ordinary_values = warm_variants(&fixture.db, &ordinary);
                let shared_values = warm_variants(&fixture.db, &shared);
                let values: Vec<_> = variants
                    .iter()
                    .map(|items| warm_variants(&fixture.db, items))
                    .collect();
                for (index, &krate) in fixture.contexts.iter().enumerate() {
                    for _ in 0..2 {
                        let keys = declared_context_keys(&mut fixture.db, krate);
                        assert!(variants[index].iter().all(|id| keys.contains(&id.as_id())));
                        assert!(
                            ordinary
                                .iter()
                                .chain(&shared)
                                .all(|id| !keys.contains(&id.as_id()))
                        );
                        fixture.db.values_in_memory_for_inputs(
                            &keys,
                            true,
                            false,
                            &[],
                            &mut |_, _| {},
                        );
                        fixture.db.spill_persisted_for_inputs(&keys);
                        assert_eq!(held_variant_values(&mut fixture.db, &variants[index]), 0);
                        let mut lists = 0;
                        fixture.db.values_in_memory_for_inputs(
                            &keys,
                            false,
                            true,
                            &["EnumVariants::of_"],
                            &mut |_, count| lists += count,
                        );
                        assert_eq!(
                            lists, 0,
                            "completed variant lists are encoded, not resident"
                        );
                    }
                    for (other, values) in values.iter().enumerate() {
                        for value in values {
                            value.assert_held(other > index);
                        }
                    }
                    for value in ordinary_values.iter().chain(&shared_values) {
                        value.assert_held(true);
                    }
                    assert_eq!(held_variant_values(&mut fixture.db, &ordinary), 15);
                    assert_eq!(held_variant_values(&mut fixture.db, &shared), 6);
                }
                assert_eq!(executions.0.load(Ordering::SeqCst), collected);
                for (index, &krate) in fixture.contexts.iter().enumerate() {
                    let (current, inactive) = declared_variants(&fixture.db, krate);
                    assert_eq!(current, variants[index]);
                    assert_eq!(inactive, count - 1);
                    warm_variants(&fixture.db, &current);
                    assert_eq!(held_variant_values(&mut fixture.db, &current), 18);
                }
                assert_eq!(
                    executions.0.load(Ordering::SeqCst),
                    collected,
                    "selection and re-entry reuse saved variant lists"
                );
            });
        }
    }
}

#[test]
fn enum_variant_selection_skips_untouched_and_stale_lists() {
    let _fixture = FIXTURE.lock().unwrap();
    let _reset = ResetStore;
    for restored in [false, true] {
        salsa::set_spill_store(Some(Arc::new(Store::default())));
        let executions = VariantExecutions::default();
        let subscriber = tracing_subscriber::registry().with(executions.clone());
        tracing::subscriber::with_default(subscriber, || {
            let mut fixture = Fixture::build("pub enum Local { Original = 1 }", 1);
            let context = fixture.contexts[0];
            crate_def_map(&fixture.db, context);
            if restored {
                fixture.restore();
            }
            declared_context_keys(&mut fixture.db, context);
            assert_eq!(executions.0.load(Ordering::SeqCst), 0);
            let (previous, _) = declared_variants(&fixture.db, context);
            assert_eq!(previous.len(), 1);
            warm_variants(&fixture.db, &previous);
            assert_eq!(executions.0.load(Ordering::SeqCst), 1);
            let mut change = FileChange::default();
            change.change_file(
                FileId::from_raw(2),
                Some("pub enum Local { Replacement = 2 }".into()),
            );
            change.apply(&mut fixture.db);
            let keys = declared_context_keys(&mut fixture.db, context);
            assert_eq!(
                executions.0.load(Ordering::SeqCst),
                1,
                "retirement does not validate stale variant lists"
            );
            assert!(previous.iter().all(|id| !keys.contains(&id.as_id())));
            let (current, _) = declared_variants(&fixture.db, context);
            assert_eq!(current.len(), 1);
            assert_ne!(current, previous);
            let values = warm_variants(&fixture.db, &current);
            let keys = declared_context_keys(&mut fixture.db, context);
            assert!(current.iter().all(|id| keys.contains(&id.as_id())));
            assert!(previous.iter().all(|id| !keys.contains(&id.as_id())));
            fixture
                .db
                .values_in_memory_for_inputs(&keys, true, false, &[], &mut |_, _| {});
            fixture.db.spill_persisted_for_inputs(&keys);
            assert_eq!(held_variant_values(&mut fixture.db, &current), 0);
            for value in values {
                value.assert_held(false);
            }
            let collected = executions.0.load(Ordering::SeqCst);
            assert_eq!(declared_variants(&fixture.db, context).0, current);
            assert_eq!(executions.0.load(Ordering::SeqCst), collected);
        });
    }
}

#[test]
fn associated_locations_retire_method_bodies_without_collecting_members() {
    let _fixture = FIXTURE.lock().unwrap();
    let _reset = ResetStore;
    for count in [1, 4, 16] {
        let mut source = String::from(
            "extern crate unused; pub use unused::{Dependency, Shared};\n\
             macro_rules! member { () => { fn generated() {} } }\n\
             pub trait Local { fn defaulted() {} fn required(); type Item; const VALUE: u8 = 1; member!(); }\n\
             pub struct LocalType;\n\
             impl LocalType { pub fn inherent() {} member!(); }\n\
             impl Local for LocalType { fn required() {} type Item = u8; const VALUE: u8 = 2; }\n",
        );
        for variant in 0..count {
            source.push_str(&format!(
                "impl LocalType {{ #[cfg(variant = \"{variant}\")] fn selected() {{ let _ = {variant}; }} }}\n"
            ));
        }
        for restored in [false, true] {
            salsa::set_spill_store(Some(Arc::new(Store::default())));
            let executions = MemberExecutions::default();
            let subscriber = tracing_subscriber::registry().with(executions.clone());
            tracing::subscriber::with_default(subscriber, || {
                let mut fixture = Fixture::build_with_dependency(
                    &source,
                    count,
                    "pub struct Dependency; pub trait Shared { fn shared() {} } impl Dependency { pub fn shared() {} }",
                );
                fixture.add_dependency();
                let dependency = ra_ap_ide_db::base_db::all_crates(&fixture.db)
                    .iter()
                    .copied()
                    .find(|krate| *krate != fixture.krate && !fixture.contexts.contains(krate))
                    .unwrap();
                let ordinary = associated_functions(&fixture.db, fixture.krate);
                assert_eq!(ordinary.len(), 6);
                let functions: Vec<_> = fixture
                    .contexts
                    .iter()
                    .map(|&krate| associated_functions(&fixture.db, krate))
                    .collect();
                assert!(functions.iter().all(|items| items.len() == 7));
                let members: Vec<_> = fixture
                    .contexts
                    .iter()
                    .map(|&krate| associated_query_keys(&fixture.db, krate))
                    .collect();
                assert!(
                    members.iter().all(|keys| keys.len() == 13),
                    "methods, consts, type aliases and macro calls"
                );
                // Shared member work can first be computed while processing a cfg copy.
                let shared = associated_functions(&fixture.db, dependency);
                assert_eq!(shared.len(), 2);
                for items in functions.iter().chain([&ordinary, &shared]) {
                    warm_items(&fixture.db, items);
                }
                let collected = executions.0.load(Ordering::SeqCst);
                assert!(collected > 0);
                if restored {
                    fixture.restore();
                }
                let ordinary_markers = warm_items(&fixture.db, &ordinary);
                let shared_markers = warm_items(&fixture.db, &shared);
                let markers: Vec<_> = functions
                    .iter()
                    .map(|items| warm_items(&fixture.db, items))
                    .collect();
                for (index, &krate) in fixture.contexts.iter().enumerate() {
                    let keys = declared_context_keys(&mut fixture.db, krate);
                    assert!(
                        functions[index].iter().all(|id| keys.contains(&id.as_id())),
                        "associated method keys must be selected"
                    );
                    assert!(members[index].iter().all(|id| keys.contains(id)));
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
                                .all(|marker| (marker.strong_count() > 0) == (other > index))
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
                    assert_eq!(
                        held_item_values(&mut fixture.db, &shared),
                        2 * shared.len() as u64
                    );
                }
                assert_eq!(
                    executions.0.load(Ordering::SeqCst),
                    collected,
                    "retirement must only decode existing member results"
                );
                for (index, &krate) in fixture.contexts.iter().enumerate() {
                    assert_eq!(associated_functions(&fixture.db, krate), functions[index]);
                    warm_items(&fixture.db, &functions[index]);
                    assert_eq!(held_item_values(&mut fixture.db, &functions[index]), 14);
                }
                assert_eq!(
                    executions.0.load(Ordering::SeqCst),
                    collected,
                    "re-entry must decode saved member lists"
                );
            });
        }
    }
}

#[test]
fn associated_location_selection_skips_untouched_and_stale_member_results() {
    let _fixture = FIXTURE.lock().unwrap();
    let _reset = ResetStore;
    for restored in [false, true] {
        salsa::set_spill_store(Some(Arc::new(Store::default())));
        let executions = MemberExecutions::default();
        let subscriber = tracing_subscriber::registry().with(executions.clone());
        tracing::subscriber::with_default(subscriber, || {
            let mut fixture = Fixture::build(
                "pub trait Local { fn original() {} } pub struct LocalType; impl LocalType { pub fn original() {} }",
                1,
            );
            let context = fixture.contexts[0];
            crate_def_map(&fixture.db, context);
            if restored {
                fixture.restore();
            }
            declared_context_keys(&mut fixture.db, context);
            assert_eq!(
                executions.0.load(Ordering::SeqCst),
                0,
                "untouched members are not collected to retire them"
            );
            let previous = associated_functions(&fixture.db, context);
            assert_eq!(previous.len(), 2);
            warm_items(&fixture.db, &previous);
            let collected = executions.0.load(Ordering::SeqCst);
            assert_eq!(collected, 2);
            let mut change = FileChange::default();
            change.change_file(FileId::from_raw(2), Some(
                "pub trait Local { fn replacement() {} } pub struct LocalType; impl LocalType { pub fn replacement() {} }".into()));
            change.apply(&mut fixture.db);
            let keys = declared_context_keys(&mut fixture.db, context);
            assert_eq!(
                executions.0.load(Ordering::SeqCst),
                collected,
                "stale members are not validated merely for retirement"
            );
            assert!(previous.iter().all(|id| !keys.contains(&id.as_id())));
            let current = associated_functions(&fixture.db, context);
            assert_eq!(current.len(), 2);
            assert_ne!(previous, current);
            let markers = warm_items(&fixture.db, &current);
            let keys = declared_context_keys(&mut fixture.db, context);
            assert!(current.iter().all(|id| keys.contains(&id.as_id())));
            assert!(previous.iter().all(|id| !keys.contains(&id.as_id())));
            fixture
                .db
                .values_in_memory_for_inputs(&keys, true, false, &[], &mut |_, _| {});
            fixture.db.spill_persisted_for_inputs(&keys);
            assert_eq!(held_item_values(&mut fixture.db, &current), 0);
            assert!(markers.iter().all(|marker| marker.strong_count() == 0));
            let collected = executions.0.load(Ordering::SeqCst);
            assert_eq!(associated_functions(&fixture.db, context), current);
            assert_eq!(executions.0.load(Ordering::SeqCst), collected);
        });
    }
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
fn local_legacy_macros_retire_without_selecting_inherited_dependency_work() {
    let _fixture = FIXTURE.lock().unwrap();
    let _reset = ResetStore;
    for count in [1, 4, 16] {
        let mut source = String::from(
            "#[macro_use] extern crate unused; use unused::shared;\n\
             macro_rules! inherited { () => {} }\n\
             macro_rules! same { () => {} }\n\
             macro_rules! same { () => {} }\n\
             macro_rules! make { () => { macro_rules! generated { () => {} } } } make!();\n\
             #[macro_use] mod nested {\n\
                 macro_rules! inside { () => {} }\n\
                 macro_rules! same { () => {} }\n\
             }\n\
             pub mod inheritor {}\n\
             #[macro_export] macro_rules! exported { () => {} }\n",
        );
        for variant in 0..count {
            source.push_str(&format!(
                "#[cfg(variant = \"{variant}\")] macro_rules! selected {{ () => {{}} }}\n"
            ));
        }
        for restored in [false, true] {
            salsa::set_spill_store(Some(Arc::new(Store::default())));
            let executions = MapExecutions::default();
            let compiled = MacroExecutions::default();
            let subscriber = tracing_subscriber::registry()
                .with(executions.clone())
                .with(compiled.clone());
            tracing::subscriber::with_default(subscriber, || {
                let mut fixture = Fixture::build_with_dependency(
                    &source,
                    count,
                    "#[macro_export] macro_rules! shared { () => {} }\n\
                     macro_rules! private { () => {} }",
                );
                fixture.add_dependency();
                let dependency = ra_ap_ide_db::base_db::all_crates(&fixture.db)
                    .iter()
                    .copied()
                    .find(|krate| *krate != fixture.krate && !fixture.contexts.contains(krate))
                    .unwrap();
                let ordinary = declared_macros(&fixture.db, fixture.krate);
                let shared = declared_macros(&fixture.db, dependency);
                assert_eq!(ordinary.len(), 8);
                assert_eq!(shared.len(), 2);
                let macros: Vec<_> = fixture
                    .contexts
                    .iter()
                    .map(|&krate| declared_macros(&fixture.db, krate))
                    .collect();
                assert!(macros.iter().all(|items| items.len() == 9));
                for items in &macros {
                    warm_macros(&fixture.db, items);
                    warm_macros(&fixture.db, &shared);
                }
                warm_macros(&fixture.db, &ordinary);
                if restored {
                    fixture.restore();
                }
                let shared_markers = warm_macros(&fixture.db, &shared);
                let ordinary_markers = warm_macros(&fixture.db, &ordinary);
                let markers: Vec<_> = macros
                    .iter()
                    .map(|items| warm_macros(&fixture.db, items))
                    .collect();
                let collected = executions.0.load(Ordering::SeqCst);
                let compiled_before = compiled.0.load(Ordering::SeqCst);
                assert_eq!(collected, count + 2);
                assert_eq!(held_expanders(&mut fixture.db, &ordinary), 8);
                assert_eq!(held_expanders(&mut fixture.db, &shared), 2);
                for items in &macros {
                    assert_eq!(held_expanders(&mut fixture.db, items), 9);
                }
                for (index, &krate) in fixture.contexts.iter().enumerate() {
                    let map = crate_def_map(&fixture.db, krate);
                    let visible_legacy = map
                        .modules()
                        .flat_map(|(_, module)| module.scope.legacy_macros())
                        .map(|(_, ids)| ids.len())
                        .sum::<usize>();
                    assert!(visible_legacy > macros[index].len());
                    assert!(
                        map.modules().any(|(_, module)| module.scope.entries().any(
                            |(_, entry)| entry.take_macros().is_some_and(|id| shared.contains(&id))
                        )),
                        "the dependency macro must actually be imported"
                    );
                    let keys = declared_context_keys(&mut fixture.db, krate);
                    assert!(macros[index].iter().all(|id| keys.contains(&id.as_id())));
                    assert!(
                        expander_keys(&fixture.db, &macros[index])
                            .iter()
                            .all(|key| keys.contains(key))
                    );
                    assert!(
                        expander_keys(&fixture.db, &ordinary)
                            .iter()
                            .chain(&expander_keys(&fixture.db, &shared))
                            .all(|key| !keys.contains(key))
                    );
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
                        assert_eq!(held_macro_values(&mut fixture.db, &macros[index]), 0);
                        assert_eq!(held_expanders(&mut fixture.db, &macros[index]), 0);
                    }
                    for (other, markers) in markers.iter().enumerate() {
                        assert!(
                            markers
                                .iter()
                                .all(|marker| { (marker.strong_count() > 0) == (other > index) })
                        );
                        assert_eq!(
                            held_macro_values(&mut fixture.db, &macros[other]),
                            if other > index { 27 } else { 0 }
                        );
                        assert_eq!(
                            held_expanders(&mut fixture.db, &macros[other]),
                            if other > index { 9 } else { 0 }
                        );
                    }
                    assert!(
                        ordinary_markers
                            .iter()
                            .chain(&shared_markers)
                            .all(|marker| marker.strong_count() > 0)
                    );
                    assert_eq!(held_macro_values(&mut fixture.db, &ordinary), 24);
                    assert_eq!(held_macro_values(&mut fixture.db, &shared), 6);
                    assert_eq!(held_expanders(&mut fixture.db, &ordinary), 8);
                    assert_eq!(held_expanders(&mut fixture.db, &shared), 2);
                    assert_eq!(
                        compiled.0.load(Ordering::SeqCst),
                        compiled_before,
                        "selection and retirement must not compile macros"
                    );
                }
                for (index, &krate) in fixture.contexts.iter().enumerate() {
                    assert_eq!(declared_macros(&fixture.db, krate), macros[index]);
                    for &mac in &macros[index] {
                        assert_eq!(
                            native_macro_payload(&fixture.db, mac).bytes,
                            vec![mac.as_id().as_bits() as u8; 4096]
                        );
                    }
                    warm_macros(&fixture.db, &macros[index]);
                    assert_eq!(held_macro_values(&mut fixture.db, &macros[index]), 27);
                    assert_eq!(held_expanders(&mut fixture.db, &macros[index]), 9);
                    assert_eq!(
                        compiled.0.load(Ordering::SeqCst),
                        compiled_before + (index + 1) * 9,
                        "re-entry compiles only this context's retired definitions"
                    );
                }
                assert_eq!(executions.0.load(Ordering::SeqCst), collected);
            });
        }
    }
}

#[test]
fn legacy_macro_selection_follows_the_current_map_after_an_edit() {
    let _fixture = FIXTURE.lock().unwrap();
    let _reset = ResetStore;
    for restored in [false, true] {
        salsa::set_spill_store(Some(Arc::new(Store::default())));
        let mut fixture = Fixture::build("macro_rules! original { () => {} }", 1);
        let context = fixture.contexts[0];
        let previous = declared_macros(&fixture.db, context);
        assert_eq!(previous.len(), 1);
        warm_macros(&fixture.db, &previous);
        if restored {
            fixture.restore();
        }
        let ordinary = declared_macros(&fixture.db, fixture.krate);
        let ordinary_markers = warm_macros(&fixture.db, &ordinary);
        let mut change = FileChange::default();
        change.change_file(
            FileId::from_raw(2),
            Some("macro_rules! replacement { () => {} }".into()),
        );
        change.apply(&mut fixture.db);
        let current = declared_macros(&fixture.db, context);
        assert_eq!(current.len(), 1);
        assert_ne!(current, previous);
        let markers = warm_macros(&fixture.db, &current);
        let keys = declared_context_keys(&mut fixture.db, context);
        let current_expanders = expander_keys(&fixture.db, &current);
        assert!(current_expanders.iter().all(|key| keys.contains(key)));
        assert!(
            expander_keys(&fixture.db, &previous)
                .iter()
                .all(|key| !keys.contains(key))
        );
        assert!(
            expander_keys(&fixture.db, &ordinary)
                .iter()
                .all(|key| !keys.contains(key))
        );
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
        assert_eq!(held_macro_values(&mut fixture.db, &current), 0);
        assert_eq!(held_expanders(&mut fixture.db, &current), 0);
        assert_eq!(held_expanders(&mut fixture.db, &ordinary), 1);
        assert!(markers.iter().all(|marker| marker.strong_count() == 0));
        assert!(
            ordinary_markers
                .iter()
                .all(|marker| marker.strong_count() > 0)
        );
        assert_eq!(declared_macros(&fixture.db, context), current);
        warm_macros(&fixture.db, &current);
        assert_eq!(held_macro_values(&mut fixture.db, &current), 3);
        assert_eq!(held_expanders(&mut fixture.db, &current), 1);
    }
}

#[test]
fn restored_macro_selection_does_not_compile_missing_expanders() {
    let _fixture = FIXTURE.lock().unwrap();
    let _reset = ResetStore;
    salsa::set_spill_store(Some(Arc::new(Store::default())));
    let compiled = MacroExecutions::default();
    let subscriber = tracing_subscriber::registry().with(compiled.clone());
    tracing::subscriber::with_default(subscriber, || {
        let mut fixture = Fixture::build("macro_rules! local { () => {} }", 1);
        let context = fixture.contexts[0];
        let macros = declared_macros(&fixture.db, context);
        assert_eq!(held_expanders(&mut fixture.db, &macros), 1);
        let before = compiled.0.load(Ordering::SeqCst);
        assert_eq!(before, 1);
        fixture.restore();
        for _ in 0..2 {
            let keys = declared_context_keys(&mut fixture.db, context);
            assert!(
                expander_keys(&fixture.db, &macros)
                    .iter()
                    .all(|key| keys.contains(key))
            );
            assert_eq!(held_expanders(&mut fixture.db, &macros), 0);
            fixture
                .db
                .values_in_memory_for_inputs(&keys, true, false, &[], &mut |_, _| {});
            fixture.db.spill_persisted_for_inputs(&keys);
            assert_eq!(
                compiled.0.load(Ordering::SeqCst),
                before,
                "selection must not compile non-persisted rules after restore"
            );
        }
        warm_macros(&fixture.db, &macros);
        assert_eq!(held_expanders(&mut fixture.db, &macros), 1);
        assert_eq!(compiled.0.load(Ordering::SeqCst), before + 1);
    });
}

#[test]
fn declarative_expander_keys_preserve_the_defining_crate_cfg() {
    let _fixture = FIXTURE.lock().unwrap();
    let mut fixture = Fixture::build(
        "#[cfg_attr(variant = \"0\", rustc_macro_transparency = \"opaque\")]\n\
         macro_rules! local { () => {} }",
        1,
    );
    let mac = declared_macros(&fixture.db, fixture.krate)[0];
    let ordinary = mac.declarative_expander_key(&fixture.db).unwrap();
    let context = ra_ap_hir_expand::declarative::DeclarativeMacroKey::new(
        &fixture.db,
        ordinary.ast_id(&fixture.db),
        fixture.contexts[0],
    );
    assert_ne!(
        ordinary, context,
        "one site in two cfg sets needs distinct memos"
    );
    assert_eq!(
        ordinary.expander(&fixture.db).transparency,
        ra_ap_hir_expand::hygiene::Transparency::SemiOpaque
    );
    assert_eq!(
        context.expander(&fixture.db).transparency,
        ra_ap_hir_expand::hygiene::Transparency::Opaque
    );
    let ordinary_key = ordinary.as_id();
    let context_key = context.as_id();
    fixture
        .db
        .values_in_memory_for_inputs(&[context_key], true, false, &[], &mut |_, _| {});
    let mut remaining = 0;
    fixture
        .db
        .values_in_memory_for_inputs(&[ordinary_key], false, true, &[], &mut |_, n| {
            remaining += n
        });
    assert_eq!(remaining, 1);
}

#[test]
fn modern_macro_expanders_preserve_expansion_after_retirement_and_edits() {
    let _fixture = FIXTURE.lock().unwrap();
    let _reset = ResetStore;
    for restored in [false, true] {
        salsa::set_spill_store(Some(Arc::new(Store::default())));
        let mut fixture = Fixture::build(
            "pub macro make() { pub fn before() {} } make!();\n\
             #[rustc_builtin_macro] macro_rules! line { () => {} }",
            1,
        );
        let context = fixture.contexts[0];
        let macros = declared_macros(&fixture.db, context);
        assert_eq!(macros.len(), 2);
        assert!(macros.iter().any(|mac| matches!(mac, MacroId::Macro2Id(_))));
        assert_eq!(
            expander_keys(&fixture.db, &macros).len(),
            1,
            "builtins must not allocate a declarative expander key"
        );
        let ordinary = declared_macros(&fixture.db, fixture.krate);
        warm_macros(&fixture.db, &macros);
        warm_macros(&fixture.db, &ordinary);
        let before = declared_functions(&fixture.db, context);
        assert_eq!(before.len(), 1);
        if restored {
            fixture.restore();
            warm_macros(&fixture.db, &macros);
            warm_macros(&fixture.db, &ordinary);
        }
        let keys = declared_context_keys(&mut fixture.db, context);
        assert!(
            expander_keys(&fixture.db, &macros)
                .iter()
                .all(|key| keys.contains(key))
        );
        assert!(
            expander_keys(&fixture.db, &ordinary)
                .iter()
                .all(|key| !keys.contains(key))
        );
        fixture
            .db
            .values_in_memory_for_inputs(&keys, true, false, &[], &mut |_, _| {});
        fixture.db.spill_persisted_for_inputs(&keys);
        assert_eq!(held_expanders(&mut fixture.db, &macros), 0);
        assert_eq!(held_expanders(&mut fixture.db, &ordinary), 1);
        assert_eq!(declared_functions(&fixture.db, context), before);
        warm_macros(&fixture.db, &macros);
        assert_eq!(held_expanders(&mut fixture.db, &macros), 1);

        let mut change = FileChange::default();
        change.change_file(
            FileId::from_raw(2),
            Some(
                "pub macro make() { pub fn after() {} pub fn added() {} } make!();\n\
             #[rustc_builtin_macro] macro_rules! line { () => {} }"
                    .into(),
            ),
        );
        change.apply(&mut fixture.db);
        let current = declared_macros(&fixture.db, context);
        warm_macros(&fixture.db, &current);
        let after = declared_functions(&fixture.db, context);
        let mut names: Vec<_> = after
            .iter()
            .map(|&id| {
                ra_ap_hir_def::signatures::FunctionSignature::of(&fixture.db, id)
                    .name
                    .display(&fixture.db, Edition::CURRENT)
                    .to_string()
            })
            .collect();
        names.sort();
        assert_eq!(names, ["added", "after"]);
        let keys = declared_context_keys(&mut fixture.db, context);
        fixture
            .db
            .values_in_memory_for_inputs(&keys, true, false, &[], &mut |_, _| {});
        fixture.db.spill_persisted_for_inputs(&keys);
        assert_eq!(held_expanders(&mut fixture.db, &current), 0);
        assert_eq!(declared_functions(&fixture.db, context), after);
        assert_eq!(declared_functions(&fixture.db, fixture.krate).len(), 1);
    }
}

/// Capture actual tree locations before restore; warming uses the normal RA queries.
#[derive(Debug)]
struct TreeLocations {
    files: Vec<HirFileId>,
    blocks: Vec<BlockId>,
}

impl TreeLocations {
    fn collect(db: &RootDatabase, krate: Crate, bodies: &LocalBodies) -> Self {
        let mut files = Vec::new();
        let maps = std::iter::once(crate_def_map(db, krate))
            .chain(bodies.blocks.iter().map(|&block| block_def_map(db, block)));
        for map in maps {
            for (_, module) in map.modules() {
                match module.origin {
                    ModuleOrigin::CrateRoot { definition }
                    | ModuleOrigin::File { definition, .. } => files.push(definition.into()),
                    ModuleOrigin::Inline {
                        definition_tree_id, ..
                    } => {
                        files.push(definition_tree_id.file_id());
                    }
                    ModuleOrigin::BlockExpr { .. } => {}
                }
                files.extend(module.scope.all_macro_calls().map(HirFileId::from));
                for declaration in module.scope.declarations() {
                    if let ModuleDefId::TraitId(id) = declaration {
                        files.extend(
                            id.trait_items(db)
                                .macro_calls
                                .iter()
                                .map(|(_, id)| HirFileId::from(*id)),
                        );
                    }
                }
                for id in module.scope.impls() {
                    files.extend(
                        id.impl_items(db)
                            .macro_calls
                            .iter()
                            .map(|(_, id)| HirFileId::from(*id)),
                    );
                }
            }
        }
        files.sort_unstable_by_key(|file| file.as_id().as_bits());
        files.dedup();
        Self {
            files,
            blocks: bodies.blocks.clone(),
        }
    }

    fn keys(&self, db: &RootDatabase, krate: Crate) -> Vec<salsa::Id> {
        self.files
            .iter()
            .map(|&file| FileItemTreeKey::new(db, file, krate).as_id())
            .chain(
                self.blocks
                    .iter()
                    .map(|&block| BlockItemTreeKey::new(db, block, krate).as_id()),
            )
            .collect()
    }

    fn warm(&self, db: &RootDatabase, krate: Crate) -> Vec<String> {
        self.files
            .iter()
            .map(|&file| file_item_tree(db, file, krate).pretty_print(db, Edition::CURRENT))
            .chain(self.blocks.iter().map(|&block| {
                BlockItemTreeKey::new(db, block, krate)
                    .item_tree(db)
                    .pretty_print(db, Edition::CURRENT)
            }))
            .collect()
    }
}

fn held_trees(db: &mut RootDatabase, keys: &[salsa::Id]) -> u64 {
    let mut held = 0;
    db.values_in_memory_for_inputs(
        keys,
        false,
        true,
        &["file_item_tree_query", "block_item_tree_query"],
        &mut |_, count| held += count,
    );
    held
}

fn all_held_trees(db: &mut RootDatabase) -> u64 {
    let mut held = 0;
    db.values_in_memory(
        false,
        true,
        &["file_item_tree_query", "block_item_tree_query"],
        &mut |_, count| held += count,
    );
    held
}

#[derive(Clone, Default)]
struct TreeExecutions(Arc<AtomicUsize>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for TreeExecutions {
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        _: &tracing::Id,
        _: tracing_subscriber::layer::Context<'_, S>,
    ) {
        if matches!(
            attrs.metadata().name(),
            "file_item_tree_query" | "block_item_tree_query"
        ) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
}

#[test]
fn composite_item_trees_retire_without_lowering_and_preserve_shared_files() {
    let _fixture = FIXTURE.lock().unwrap();
    let _reset = ResetStore;
    for count in [1, 4, 16] {
        for restored in [false, true] {
            salsa::set_spill_store(Some(Arc::new(Store::default())));
            let executions = TreeExecutions::default();
            let subscriber = tracing_subscriber::registry().with(executions.clone());
            tracing::subscriber::with_default(subscriber, || {
                let mut fixture = Fixture::build_with_dependency(
                    "extern crate unused; use unused::shared;\n\
                     #[path = \"../1/lib.rs\"] mod file_module;\n\
                     pub mod inline { pub fn inside() {} }\n\
                     macro_rules! empty { () => {} } empty!();\n\
                     macro_rules! make { () => { pub fn generated() {} empty!(); } } make!();\n\
                     macro_rules! associated { () => { fn method() {} } }\n\
                     pub trait Local { associated!(); empty!(); }\n\
                     pub struct LocalType; impl LocalType { associated!(); empty!(); }\n\
                     pub fn outer() { fn inner() {} { fn nested() {} }\n\
                         macro_rules! local { () => { fn generated_local() {} } } local!(); }",
                    count,
                    "pub fn shared() {} #[cfg(variant = \"0\")] pub fn selected() {}",
                );
                fixture.add_dependency();
                let dependency = ra_ap_ide_db::base_db::all_crates(&fixture.db)
                    .iter()
                    .copied()
                    .find(|krate| *krate != fixture.krate && !fixture.contexts.contains(krate))
                    .unwrap();
                let crates: Vec<_> = [fixture.krate, dependency]
                    .into_iter()
                    .chain(fixture.contexts.iter().copied())
                    .collect();
                let bodies: Vec<_> = crates
                    .iter()
                    .map(|&krate| LocalBodies::collect(&fixture.db, krate))
                    .collect();
                let trees: Vec<_> = crates
                    .iter()
                    .zip(&bodies)
                    .map(|(&krate, body)| TreeLocations::collect(&fixture.db, krate, body))
                    .collect();
                let expected: Vec<_> = crates
                    .iter()
                    .zip(&trees)
                    .map(|(&krate, tree)| tree.warm(&fixture.db, krate))
                    .collect();
                assert!(
                    trees[2].files.len() >= 5,
                    "root, shared file, generated and empty expansions"
                );
                assert!(
                    trees[2].blocks.len() >= 2,
                    "body-local and nested block trees"
                );
                if restored {
                    fixture.restore();
                    for ((&krate, body), tree) in crates.iter().zip(&bodies).zip(&trees) {
                        assert_eq!(LocalBodies::collect(&fixture.db, krate), *body);
                        tree.warm(&fixture.db, krate);
                    }
                }
                let keys: Vec<_> = crates
                    .iter()
                    .zip(&trees)
                    .map(|(&krate, tree)| tree.keys(&fixture.db, krate))
                    .collect();
                for ids in &keys {
                    assert_eq!(held_trees(&mut fixture.db, ids), ids.len() as u64);
                }
                assert_eq!(
                    all_held_trees(&mut fixture.db),
                    keys.iter().map(|ids| ids.len() as u64).sum::<u64>(),
                    "every resident item tree in the fixture has an enumerated owner"
                );
                let before = executions.0.load(Ordering::SeqCst);
                for index in 2..crates.len() {
                    for _ in 0..2 {
                        let selected = declared_context_keys(&mut fixture.db, crates[index]);
                        assert!(
                            keys[index].iter().all(|key| selected.contains(key)),
                            "missing tree keys: count={count} restored={restored} index={index}"
                        );
                        for (other, ids) in keys.iter().enumerate() {
                            if other != index {
                                assert!(ids.iter().all(|key| !selected.contains(key)));
                            }
                        }
                        fixture.db.values_in_memory_for_inputs(
                            &selected,
                            true,
                            false,
                            &[],
                            &mut |_, _| {},
                        );
                        fixture.db.spill_persisted_for_inputs(&selected);
                        assert_eq!(held_trees(&mut fixture.db, &keys[index]), 0);
                        assert_eq!(
                            executions.0.load(Ordering::SeqCst),
                            before,
                            "selection and repeated retirement must not lower trees"
                        );
                    }
                    for (other, ids) in keys.iter().enumerate() {
                        let retained = other < 2 || other > index;
                        assert_eq!(
                            held_trees(&mut fixture.db, ids),
                            if retained { ids.len() as u64 } else { 0 }
                        );
                    }
                    let remaining: u64 = keys
                        .iter()
                        .enumerate()
                        .filter(|(other, _)| *other < 2 || *other > index)
                        .map(|(_, ids)| ids.len() as u64)
                        .sum();
                    assert_eq!(all_held_trees(&mut fixture.db), remaining);
                }
                for (index, (&krate, tree)) in crates.iter().zip(&trees).enumerate() {
                    assert_eq!(tree.warm(&fixture.db, krate), expected[index]);
                    assert_eq!(
                        held_trees(&mut fixture.db, &keys[index]),
                        keys[index].len() as u64
                    );
                }
            });
        }
    }
}

#[test]
fn restored_item_tree_selection_does_not_lower_missing_values() {
    let _fixture = FIXTURE.lock().unwrap();
    let _reset = ResetStore;
    salsa::set_spill_store(Some(Arc::new(Store::default())));
    let mut fixture = Fixture::build(
        "macro_rules! empty { () => {} } empty!(); pub mod inline { pub fn inside() {} }",
        1,
    );
    let context = fixture.contexts[0];
    let bodies = LocalBodies::collect(&fixture.db, context);
    let trees = TreeLocations::collect(&fixture.db, context, &bodies);
    assert!(trees.files.len() >= 2);
    trees.warm(&fixture.db, context);
    fixture.restore();
    let executions = TreeExecutions::default();
    let subscriber = tracing_subscriber::registry().with(executions.clone());
    tracing::subscriber::with_default(subscriber, || {
        for _ in 0..2 {
            let selected = declared_context_keys(&mut fixture.db, context);
            let keys = trees.keys(&fixture.db, context);
            assert!(keys.iter().all(|key| selected.contains(key)));
            assert_eq!(held_trees(&mut fixture.db, &keys), 0);
            assert_eq!(executions.0.load(Ordering::SeqCst), 0);
        }
        let before = trees.warm(&fixture.db, context);
        let selected = declared_context_keys(&mut fixture.db, context);
        fixture
            .db
            .values_in_memory_for_inputs(&selected, true, false, &[], &mut |_, _| {});
        assert_eq!(trees.warm(&fixture.db, context), before);
    });
    let file = context.root_file_id(&fixture.db);
    let mut change = FileChange::default();
    change.change_file(
        file.file_id(&fixture.db),
        Some("pub fn replacement() {}".into()),
    );
    change.apply(&mut fixture.db);
    let current = file_item_tree(&fixture.db, file.into(), context)
        .pretty_print(&fixture.db, Edition::CURRENT);
    assert!(current.contains("replacement"));
    let selected = declared_context_keys(&mut fixture.db, context);
    let root_key = FileItemTreeKey::new(&fixture.db, HirFileId::from(file), context).as_id();
    assert!(selected.contains(&root_key));
    fixture
        .db
        .values_in_memory_for_inputs(&selected, true, false, &[], &mut |_, _| {});
    assert_eq!(held_trees(&mut fixture.db, &[root_key]), 0);
    assert_eq!(
        file_item_tree(&fixture.db, file.into(), context)
            .pretty_print(&fixture.db, Edition::CURRENT),
        current
    );
}

#[test]
fn body_local_declarations_retire_real_bodies_through_current_block_maps() {
    let _fixture = FIXTURE.lock().unwrap();
    let _reset = ResetStore;
    for count in [1, 4, 16] {
        let mut source = String::from(
            "extern crate unused; use unused::shared;\n\
             pub fn outer() {\n\
                 fn inner() { { fn deepest() {} deepest(); } }\n\
                 const LOCAL: usize = { fn from_const() {} 1 };\n\
                 static STATIC: usize = { fn from_static() {} 2 };\n\
                 trait Local { fn defaulted() { fn from_method() {} } }\n\
                 struct LocalType;\n\
                 impl LocalType { fn inherent() { fn inside_impl() {} } }\n\
                 impl Local for LocalType {}\n\
                 enum LocalEnum { A = { fn from_variant() {} 1 } }\n\
                 macro_rules! make { () => { fn generated() {} } } make!();\n\
                 { fn nested() {} }\n",
        );
        for variant in 0..count {
            source.push_str(&format!(
                "#[cfg(variant = \"{variant}\")] fn selected() {{ fn inside_cfg() {{}} }}\n"
            ));
        }
        source.push_str("inner(); }");
        for restored in [false, true] {
            salsa::set_spill_store(Some(Arc::new(Store::default())));
            let executions = LocalExecutions::default();
            let subscriber = tracing_subscriber::registry().with(executions.clone());
            tracing::subscriber::with_default(subscriber, || {
                let mut fixture = Fixture::build(&source, count);
                fixture.add_dependency();
                let dependency = ra_ap_ide_db::base_db::all_crates(&fixture.db)
                    .iter()
                    .copied()
                    .find(|krate| *krate != fixture.krate && !fixture.contexts.contains(krate))
                    .unwrap();
                let ordinary = LocalBodies::collect(&fixture.db, fixture.krate);
                let shared = LocalBodies::collect(&fixture.db, dependency);
                let locals: Vec<_> = fixture
                    .contexts
                    .iter()
                    .map(|&krate| LocalBodies::collect(&fixture.db, krate))
                    .collect();
                assert_eq!(ordinary.bodies.len(), 15);
                assert_eq!(shared.bodies.len(), 1);
                assert!(locals.iter().all(|facts| facts.bodies.len() == 17));
                assert!(locals.iter().all(|facts| facts.macros.len() == 1));
                if restored {
                    fixture.restore();
                    for (index, &krate) in fixture.contexts.iter().enumerate() {
                        assert_eq!(LocalBodies::collect(&fixture.db, krate), locals[index]);
                    }
                    assert_eq!(LocalBodies::collect(&fixture.db, fixture.krate), ordinary);
                    assert_eq!(LocalBodies::collect(&fixture.db, dependency), shared);
                }
                let ordinary_values = ordinary.warm(&fixture.db);
                let shared_values = shared.warm(&fixture.db);
                let values: Vec<_> = locals.iter().map(|facts| facts.warm(&fixture.db)).collect();
                warm_macros(&fixture.db, &ordinary.macros);
                for facts in &locals {
                    warm_macros(&fixture.db, &facts.macros);
                    assert_eq!(held_expanders(&mut fixture.db, &facts.macros), 1);
                }
                for (index, facts) in locals.iter().enumerate() {
                    let blocks: Vec<_> = facts.blocks.iter().map(AsId::as_id).collect();
                    fixture.db.spill_persisted_for_inputs(&blocks);
                    assert_eq!(
                        fixture.db.cached_query_keys(&blocks, &["block_def_map"]),
                        blocks,
                        "current block maps before retirement: count={count} restored={restored} context={index}"
                    );
                    let bodies: Vec<_> = facts.bodies.iter().map(AsId::as_id).collect();
                    let outputs = fixture.db.tracked_output_keys(&bodies);
                    assert!(
                        blocks.iter().all(|id| outputs.contains(id)),
                        "body output records before retirement: count={count} restored={restored} context={index}"
                    );
                }
                let collected_blocks = executions.blocks.load(Ordering::SeqCst);
                let collected_bodies = executions.bodies.load(Ordering::SeqCst);
                assert!(collected_blocks > count);
                for (index, &krate) in fixture.contexts.iter().enumerate() {
                    for retirement in 0..2 {
                        let keys = declared_context_keys(&mut fixture.db, krate);
                        assert!(
                            locals[index]
                                .bodies
                                .iter()
                                .all(|id| keys.contains(&id.as_id())),
                            "count={count} restored={restored} context={index} retirement={retirement}; missing {:?}",
                            locals[index]
                                .bodies
                                .iter()
                                .filter(|id| !keys.contains(&id.as_id()))
                                .collect::<Vec<_>>()
                        );
                        assert!(
                            locals[index]
                                .blocks
                                .iter()
                                .all(|id| keys.contains(&id.as_id()))
                        );
                        assert!(
                            locals[index]
                                .macros
                                .iter()
                                .all(|id| keys.contains(&id.as_id()))
                        );
                        assert!(
                            expander_keys(&fixture.db, &locals[index].macros)
                                .iter()
                                .all(|key| keys.contains(key))
                        );
                        assert!(
                            expander_keys(&fixture.db, &ordinary.macros)
                                .iter()
                                .all(|key| !keys.contains(key))
                        );
                        assert!(
                            ordinary
                                .bodies
                                .iter()
                                .chain(&shared.bodies)
                                .all(|id| !keys.contains(&id.as_id()))
                        );
                        fixture.db.values_in_memory_for_inputs(
                            &keys,
                            true,
                            false,
                            &[],
                            &mut |_, _| {},
                        );
                        fixture.db.spill_persisted_for_inputs(&keys);
                        assert_eq!(locals[index].held(&mut fixture.db), 0);
                        assert_eq!(held_expanders(&mut fixture.db, &locals[index].macros), 0);
                        assert_eq!(held_expanders(&mut fixture.db, &ordinary.macros), 1);
                        let mut held_blocks = 0;
                        fixture.db.values_in_memory_for_inputs(
                            &keys,
                            false,
                            true,
                            &["block_def_map"],
                            &mut |_, count| held_blocks += count,
                        );
                        assert_eq!(held_blocks, 0);
                        assert_eq!(executions.blocks.load(Ordering::SeqCst), collected_blocks);
                        assert_eq!(executions.bodies.load(Ordering::SeqCst), collected_bodies);
                    }
                    for (other, bodies) in values.iter().enumerate() {
                        assert!(bodies.iter().all(|body| {
                            (triomphe::Arc::strong_count(body) > 1) == (other > index)
                        }));
                        assert_eq!(
                            locals[other].held(&mut fixture.db),
                            if other > index { 34 } else { 0 }
                        );
                        assert_eq!(
                            held_expanders(&mut fixture.db, &locals[other].macros),
                            u64::from(other > index)
                        );
                    }
                    assert!(
                        ordinary_values
                            .iter()
                            .chain(&shared_values)
                            .all(|body| triomphe::Arc::strong_count(body) > 1)
                    );
                    assert_eq!(ordinary.held(&mut fixture.db), 30);
                    assert_eq!(shared.held(&mut fixture.db), 2);
                }
                for (index, &krate) in fixture.contexts.iter().enumerate() {
                    assert_eq!(LocalBodies::collect(&fixture.db, krate), locals[index]);
                    locals[index].warm(&fixture.db);
                    assert_eq!(locals[index].held(&mut fixture.db), 34);
                    warm_macros(&fixture.db, &locals[index].macros);
                    assert_eq!(held_expanders(&mut fixture.db, &locals[index].macros), 1);
                }
                assert_eq!(executions.blocks.load(Ordering::SeqCst), collected_blocks);
            });
        }
    }
}

#[test]
fn body_local_selection_skips_untouched_and_stale_owners() {
    let _fixture = FIXTURE.lock().unwrap();
    let _reset = ResetStore;
    for restored in [false, true] {
        salsa::set_spill_store(Some(Arc::new(Store::default())));
        let executions = LocalExecutions::default();
        let subscriber = tracing_subscriber::registry().with(executions.clone());
        tracing::subscriber::with_default(subscriber, || {
            let mut fixture = Fixture::build("pub fn outer() { fn original() {} }", 1);
            let context = fixture.contexts[0];
            crate_def_map(&fixture.db, context);
            if restored {
                fixture.restore();
            }
            declared_context_keys(&mut fixture.db, context);
            assert_eq!(executions.blocks.load(Ordering::SeqCst), 0);
            assert_eq!(executions.bodies.load(Ordering::SeqCst), 0);
            let previous = LocalBodies::collect(&fixture.db, context);
            let outer: DefWithBodyId = declared_functions(&fixture.db, context)[0].into();
            assert_eq!(previous.bodies.len(), 2);
            assert_eq!(previous.blocks.len(), 1);
            let collected_blocks = executions.blocks.load(Ordering::SeqCst);
            let collected_bodies = executions.bodies.load(Ordering::SeqCst);
            let mut change = FileChange::default();
            change.change_file(
                FileId::from_raw(2),
                Some("pub fn outer() { fn replacement() {} }".into()),
            );
            change.apply(&mut fixture.db);
            let keys = declared_context_keys(&mut fixture.db, context);
            assert_eq!(executions.blocks.load(Ordering::SeqCst), collected_blocks);
            assert_eq!(executions.bodies.load(Ordering::SeqCst), collected_bodies);
            // The outer location can survive an edit, but its stale local list is skipped.
            assert!(
                previous
                    .bodies
                    .iter()
                    .filter(|&&id| id != outer)
                    .all(|id| !keys.contains(&id.as_id()))
            );
            let current = LocalBodies::collect(&fixture.db, context);
            assert_ne!(previous, current);
            assert_eq!(current.bodies.len(), 2);
            let values = current.warm(&fixture.db);
            let collected_blocks = executions.blocks.load(Ordering::SeqCst);
            let collected_bodies = executions.bodies.load(Ordering::SeqCst);
            let keys = declared_context_keys(&mut fixture.db, context);
            assert!(current.bodies.iter().all(|id| keys.contains(&id.as_id())));
            fixture
                .db
                .values_in_memory_for_inputs(&keys, true, false, &[], &mut |_, _| {});
            fixture.db.spill_persisted_for_inputs(&keys);
            assert_eq!(current.held(&mut fixture.db), 0);
            assert!(
                values
                    .iter()
                    .all(|body| triomphe::Arc::strong_count(body) == 1)
            );
            assert_eq!(executions.blocks.load(Ordering::SeqCst), collected_blocks);
            assert_eq!(executions.bodies.load(Ordering::SeqCst), collected_bodies);
        });
    }
}

#[test]
fn restored_body_local_selection_does_not_rebuild_missing_producers() {
    let _fixture = FIXTURE.lock().unwrap();
    let _reset = ResetStore;
    salsa::set_spill_store(Some(Arc::new(Store::default())));
    let executions = LocalExecutions::default();
    let subscriber = tracing_subscriber::registry().with(executions.clone());
    tracing::subscriber::with_default(subscriber, || {
        let mut fixture = Fixture::build("pub fn outer() { fn inner() {} }", 1);
        let context = fixture.contexts[0];
        let previous = LocalBodies::collect(&fixture.db, context);
        let outer: DefWithBodyId = declared_functions(&fixture.db, context)[0].into();
        assert_eq!(previous.bodies.len(), 2);
        assert_eq!(previous.blocks.len(), 1);
        fixture.restore();
        let block_keys: Vec<_> = previous.blocks.iter().map(AsId::as_id).collect();
        assert_eq!(
            fixture
                .db
                .cached_query_keys(&block_keys, &["block_def_map"]),
            block_keys
        );
        let collected_blocks = executions.blocks.load(Ordering::SeqCst);
        let collected_bodies = executions.bodies.load(Ordering::SeqCst);
        let keys = declared_context_keys(&mut fixture.db, context);
        assert!(
            previous
                .bodies
                .iter()
                .filter(|&&id| id != outer)
                .all(|id| !keys.contains(&id.as_id()))
        );
        assert!(
            block_keys.iter().all(|id| !keys.contains(id)),
            "persisted maps alone do not recover missing non-persisted producer records"
        );
        assert_eq!(executions.blocks.load(Ordering::SeqCst), collected_blocks);
        assert_eq!(executions.bodies.load(Ordering::SeqCst), collected_bodies);
        assert_eq!(LocalBodies::collect(&fixture.db, context), previous);
        let collected_bodies = executions.bodies.load(Ordering::SeqCst);
        let keys = declared_context_keys(&mut fixture.db, context);
        assert!(previous.bodies.iter().all(|id| keys.contains(&id.as_id())));
        assert_eq!(executions.blocks.load(Ordering::SeqCst), collected_blocks);
        assert_eq!(executions.bodies.load(Ordering::SeqCst), collected_bodies);
    });
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

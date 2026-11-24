//! What toggling one cfg atom of a crate in place re-executes.
//!
//! A context could be the same crate with some atoms flipped, if what rust-analyzer computes named
//! the atoms it tested: flip one and only what tested it would run again, and what ran again to the
//! same answer would stop the change there. These tests flip an atom of one crate in place and count
//! the item trees that execute again.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use ra_ap_cfg::CfgOptions;
use ra_ap_hir_def::expr_store::body::Body;
use ra_ap_hir_def::nameres::crate_def_map;
use ra_ap_hir_def::{DefWithBodyId, ModuleDefId};
use ra_ap_hir_expand::intern::Symbol;
use ra_ap_hir_ty::method_resolution::{InherentImpls, TraitImpls};
use ra_ap_ide_db::RootDatabase;
use ra_ap_ide_db::base_db::{
    AbsPathBuf, Crate, CrateGraphBuilder, CrateOrigin, CrateWorkspaceData, Env, FileChange, FileId,
    FileSet, SourceRoot, VfsPath,
};
use ra_ap_ide_db::span::Edition;
use salsa::Setter;
use tracing_subscriber::layer::SubscriberExt;

static SERIAL: Mutex<()> = Mutex::new(());

/// Counts the item trees that execute.
#[derive(Clone, Default)]
struct ItemTrees(Arc<AtomicUsize>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for ItemTrees {
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        _: &tracing::Id,
        _: tracing_subscriber::layer::Context<'_, S>,
    ) {
        if attrs.metadata().name() == "file_item_tree_query" {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
}

fn flag(cfg: &mut CfgOptions, name: &str) {
    cfg.insert_key_value(Symbol::intern("feature"), Symbol::intern(name));
}

/// A crate of `files`, the first being the root, with `cfg` set.
fn crate_of(files: &[(&str, &str)], cfg: CfgOptions) -> (RootDatabase, Crate) {
    let mut db = RootDatabase::default();
    let mut graph = CrateGraphBuilder::default();
    let mut set = FileSet::default();
    let mut change = FileChange::default();
    for (id, (name, text)) in files.iter().enumerate() {
        let file = FileId::from_raw(u32::try_from(id).unwrap());
        set.insert(file, VfsPath::new_virtual_path(format!("/{name}")));
        change.change_file(file, Some((*text).to_owned()));
    }
    let root = graph.add_crate_root(
        FileId::from_raw(0),
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
    );
    change.set_roots(vec![SourceRoot::new_local(set)]);
    change.set_crate_graph(graph);
    let crates = change.apply(&mut db).unwrap();
    let krate = crates[&root];
    (db, krate)
}

const FILES: [(&str, &str); 3] = [
    ("lib.rs", "mod one;\nmod two;\n"),
    ("one.rs", "#[cfg(feature = \"a\")]\npub fn f() {}\n"),
    ("two.rs", "#[cfg(feature = \"b\")]\npub fn g() {}\n"),
];

fn declared(db: &RootDatabase, krate: Crate) -> usize {
    crate_def_map(db, krate)
        .modules(db)
        .map(|(_, module)| module.scope.declarations().count())
        .sum()
}

#[test]
fn flipping_an_atom_runs_again_only_the_item_trees_that_test_it() {
    let _serial = SERIAL.lock().unwrap();
    let count = ItemTrees::default();
    let subscriber = tracing_subscriber::registry().with(count.clone());
    tracing::subscriber::with_default(subscriber, || {
        let (mut db, krate) = crate_of(&FILES, CfgOptions::default());
        assert_eq!(declared(&db, krate), 2, "the two modules, and neither gate is on");
        assert_eq!(count.0.load(Ordering::SeqCst), 3, "one item tree a file");

        let mut cfg = CfgOptions::default();
        flag(&mut cfg, "a");
        krate.set_cfg_options(&mut db).to(cfg);
        assert_eq!(declared(&db, krate), 3, "`a` makes `f` visible");
        assert_eq!(
            count.0.load(Ordering::SeqCst),
            4,
            "only `one.rs` tests `a`: the others' item trees are not executed again"
        );
    });
}

#[test]
fn flipping_an_atom_no_file_tests_runs_no_item_tree_again() {
    let _serial = SERIAL.lock().unwrap();
    let count = ItemTrees::default();
    let subscriber = tracing_subscriber::registry().with(count.clone());
    tracing::subscriber::with_default(subscriber, || {
        let (mut db, krate) = crate_of(&FILES, CfgOptions::default());
        assert_eq!(declared(&db, krate), 2);
        let before = count.0.load(Ordering::SeqCst);

        let mut cfg = CfgOptions::default();
        flag(&mut cfg, "unrelated");
        krate.set_cfg_options(&mut db).to(cfg);
        assert_eq!(declared(&db, krate), 2, "nothing is gated by it");
        assert_eq!(
            count.0.load(Ordering::SeqCst),
            before,
            "no item tree asked about the atom, so none is executed again"
        );
    });
}

/// Every function's body, as an analysis of the crate reads them.
fn read_bodies(db: &RootDatabase, krate: Crate) -> usize {
    let functions: Vec<_> = crate_def_map(db, krate)
        .modules(db)
        .flat_map(|(_, module)| module.scope.declarations().collect::<Vec<_>>())
        .filter_map(|def| match def {
            ModuleDefId::FunctionId(id) => Some(DefWithBodyId::from(id)),
            _ => None,
        })
        .collect();
    for &function in &functions {
        let _ = Body::with_source_map(db, function);
    }
    functions.len()
}

/// What an analysis reads of the whole crate: its impls and its lang items.
fn read_impls(db: &RootDatabase, krate: Crate) {
    let _ = InherentImpls::for_crate(db, krate);
    let _ = TraitImpls::for_crate(db, krate);
    let _ = ra_ap_hir_def::lang_item::lang_items(db, krate);
}

/// Counts every query that executes, by name.
#[derive(Clone, Default)]
struct Executed(Arc<Mutex<std::collections::BTreeMap<String, usize>>>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Executed {
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        _: &tracing::Id,
        _: tracing_subscriber::layer::Context<'_, S>,
    ) {
        *self
            .0
            .lock()
            .unwrap()
            .entry(attrs.metadata().name().to_owned())
            .or_default() += 1;
    }
}

/// `modules` files, each with a function whose body calls a macro, and one of them gated by `a`.
fn many(modules: usize) -> Vec<(String, String)> {
    let mut files = vec![(
        "lib.rs".to_owned(),
        (0..modules)
            .map(|m| format!("mod m{m};\n"))
            .collect::<String>()
            + "macro_rules! twice { ($e:expr) => { $e + $e }; }\n",
    )];
    for m in 0..modules {
        let gate = if m == 0 { "#[cfg(feature = \"a\")]\n" } else { "" };
        files.push((
            format!("m{m}.rs"),
            format!(
                "{gate}pub fn f{m}() -> u32 {{ 1 + 1 }}\npub struct S{m}(pub u32);\n\
                 impl S{m} {{ pub fn get(&self) -> u32 {{ self.0 }} }}\n\
                 impl Clone for S{m} {{ fn clone(&self) -> Self {{ S{m}(self.0) }} }}\n"
            ),
        ));
    }
    files
}

fn executed_by_flipping(modules: usize) -> std::collections::BTreeMap<String, usize> {
    let executed = Executed::default();
    let subscriber = tracing_subscriber::registry().with(executed.clone());
    tracing::subscriber::with_default(subscriber, || {
        let files = many(modules);
        let borrowed: Vec<(&str, &str)> = files
            .iter()
            .map(|(name, text)| (name.as_str(), text.as_str()))
            .collect();
        let (mut db, krate) = crate_of(&borrowed, CfgOptions::default());
        let _ = read_bodies(&db, krate);
        read_impls(&db, krate);
        let before = executed.0.lock().unwrap().clone();

        let mut cfg = CfgOptions::default();
        flag(&mut cfg, "a");
        krate.set_cfg_options(&mut db).to(cfg);
        let _ = read_bodies(&db, krate);
        read_impls(&db, krate);
        let after = executed.0.lock().unwrap().clone();
        after
            .into_iter()
            .filter_map(|(name, n)| {
                let delta = n - before.get(&name).copied().unwrap_or(0);
                (delta > 0).then_some((name, delta))
            })
            .collect()
    })
}

#[test]
fn what_a_flip_runs_again_does_not_grow_with_the_files_it_does_not_reach() {
    let _serial = SERIAL.lock().unwrap();
    let small = executed_by_flipping(4);
    let large = executed_by_flipping(40);
    tracing::info!(?small, ?large, "what flipping an atom executed again, in 4 and in 40 modules");
    assert_eq!(
        small.get("file_item_tree_query"),
        large.get("file_item_tree_query"),
        "the item trees executed again are the ones that test the atom"
    );
    assert_eq!(
        small.get("body_with_source_map_query"),
        large.get("body_with_source_map_query"),
        "the bodies executed again are the ones that read what the atom changed, not every body"
    );
    assert_eq!(small.get("inherent_impls_in_crate_query"), Some(&2), "the inherent and trait impls are collected again: {small:?}");
    assert_eq!(small, large, "everything the flip executes again is the same in a crate ten times the size");
    assert_eq!(
        large.get("body_with_source_map_query"),
        Some(&1),
        "only the body the flip makes visible is executed: no other reads the module the gate is in"
    );
}

const GATED: [(&str, &str); 3] = [
    (
        "lib.rs",
        "#[cfg(feature = \"a\")]\nmod proofs;\n#[cfg(feature = \"a\")]\npub fn gated_fn() {}\npub fn plain() {}\n",
    ),
    ("proofs.rs", "pub fn inside() {}\n"),
    ("unused.rs", ""),
];

/// The functions of the crate by name, as the definition map declares them now.
fn function_ids(db: &RootDatabase, krate: Crate) -> std::collections::BTreeMap<String, ra_ap_hir_def::FunctionId> {
    use ra_ap_hir_def::signatures::FunctionSignature;
    crate_def_map(db, krate)
        .modules(db)
        .flat_map(|(_, module)| module.scope.declarations().collect::<Vec<_>>())
        .filter_map(|def| match def {
            ModuleDefId::FunctionId(id) => Some((FunctionSignature::of(db, id).name.as_str().to_owned(), id)),
            _ => None,
        })
        .collect()
}

#[test]
fn an_item_says_why_the_definition_map_no_longer_declares_it() {
    use ra_ap_hir_def::declaration::{Absence, Declaration, function_declaration};
    let _serial = SERIAL.lock().unwrap();
    let mut on = CfgOptions::default();
    flag(&mut on, "a");
    let (mut db, krate) = crate_of(&GATED, on);
    let ids = function_ids(&db, krate);
    assert_eq!(ids.keys().map(String::as_str).collect::<Vec<_>>(), ["gated_fn", "inside", "plain"]);
    for id in ids.values() {
        assert_eq!(function_declaration(&db, *id), Declaration::Declared);
    }

    // The atom is off and the map is made again: the function is not declared by a module that is there, and
    // the other is in a module that is deleted, which is not read.
    krate.set_cfg_options(&mut db).to(CfgOptions::default());
    let _ = crate_def_map(&db, krate);
    assert_eq!(function_declaration(&db, ids["plain"]), Declaration::Declared);
    assert_eq!(
        function_declaration(&db, ids["gated_fn"]),
        Declaration::Absent(Absence::NotDeclared)
    );
    assert_eq!(
        function_declaration(&db, ids["inside"]),
        Declaration::Absent(Absence::ModuleGone(salsa::Existence::Deleted))
    );
}

const MEMBERS: [(&str, &str); 2] = [
    (
        "lib.rs",
        "#[cfg(feature = \"a\")]\nmod proofs;\npub struct Plain;\nimpl Plain {}\n#[cfg(feature = \"a\")]\nimpl Plain { fn gated(&self) {} }\n",
    ),
    ("proofs.rs", "pub trait Prove {}\npub enum Choice { A }\npub struct Witness;\nimpl Witness {}\n"),
];

#[test]
fn an_impl_a_trait_and_an_enum_say_why_the_definition_map_no_longer_declares_them() {
    use ra_ap_hir_def::declaration::{
        Absence, Declaration, enum_declaration, impl_declaration, trait_declaration,
    };
    use ra_ap_hir_def::{AdtId, ImplId};
    let _serial = SERIAL.lock().unwrap();
    let mut on = CfgOptions::default();
    flag(&mut on, "a");
    let (mut db, krate) = crate_of(&MEMBERS, on);
    let (mut impls, mut traits, mut enums) = (Vec::<ImplId>::new(), Vec::new(), Vec::new());
    for (_, module) in crate_def_map(&db, krate).modules(&db) {
        impls.extend(module.scope.impls());
        for def in module.scope.declarations() {
            match def {
                ModuleDefId::TraitId(id) => traits.push(id),
                ModuleDefId::AdtId(AdtId::EnumId(id)) => enums.push(id),
                _ => {}
            }
        }
    }
    assert_eq!((impls.len(), traits.len(), enums.len()), (3, 1, 1));
    for id in &impls {
        assert_eq!(impl_declaration(&db, *id), Declaration::Declared);
    }

    krate.set_cfg_options(&mut db).to(CfgOptions::default());
    let _ = crate_def_map(&db, krate);
    let mut why: Vec<_> = impls.iter().map(|id| impl_declaration(&db, *id)).collect();
    why.sort_by_key(|declaration| format!("{declaration:?}"));
    assert_eq!(
        why,
        vec![
            Declaration::Absent(Absence::ModuleGone(salsa::Existence::Deleted)),
            Declaration::Absent(Absence::NotDeclared),
            Declaration::Declared,
        ],
        "the impl in the module that is gone, the one the cfg no longer declares, and the one that remains"
    );
    assert_eq!(
        trait_declaration(&db, traits[0]),
        Declaration::Absent(Absence::ModuleGone(salsa::Existence::Deleted))
    );
    assert_eq!(
        enum_declaration(&db, enums[0]),
        Declaration::Absent(Absence::ModuleGone(salsa::Existence::Deleted))
    );
}

#[test]
fn flipping_an_atom_a_crate_never_asks_about_does_not_make_its_definition_map_again() {
    let _serial = SERIAL.lock().unwrap();
    let executed = Executed::default();
    let subscriber = tracing_subscriber::registry().with(executed.clone());
    tracing::subscriber::with_default(subscriber, || {
        // `one.rs` and `two.rs` have items that are cfg-disabled, which the map says in a diagnostic.
        let (mut db, krate) = crate_of(&FILES, CfgOptions::default());
        assert_eq!(declared(&db, krate), 2);
        let before = executed.0.lock().unwrap().get("crate_def_map_query").copied().unwrap_or(0);
        assert_eq!(before, 1);

        let mut cfg = CfgOptions::default();
        flag(&mut cfg, "unrelated");
        krate.set_cfg_options(&mut db).to(cfg);
        assert_eq!(declared(&db, krate), 2);
        assert_eq!(
            executed.0.lock().unwrap().get("crate_def_map_query").copied().unwrap_or(0),
            before,
            "nothing in the crate asks about `unrelated`, so its map is not made again: a diagnostic that holds the \
             crate's whole set of options depends on all of them"
        );
    });
}

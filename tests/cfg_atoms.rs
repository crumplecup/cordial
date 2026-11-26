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
    AbsPathBuf, Crate, CrateGraphBuilder, CrateName, CrateOrigin, CrateWorkspaceData, DependencyBuilder, Env,
    FileChange, FileId, FileSet, SourceRoot, VfsPath,
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
        Declaration::Absent(Absence::NotDeclared)
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
            Declaration::Absent(Absence::NotDeclared),
            Declaration::Absent(Absence::NotDeclared),
            Declaration::Declared,
        ],
        "the impl in the module the map no longer has, the one the cfg no longer declares, and the one that remains"
    );
    assert_eq!(
        trait_declaration(&db, traits[0]),
        Declaration::Absent(Absence::NotDeclared)
    );
    assert_eq!(
        enum_declaration(&db, enums[0]),
        Declaration::Absent(Absence::NotDeclared)
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

/// What each definition map built says of how many of its modules it walked and how many it took from an earlier
/// map of the same crate.
#[derive(Clone, Default)]
struct MapsBuilt(Arc<Mutex<Vec<(u64, u64)>>>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for MapsBuilt {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        #[derive(Default)]
        struct Fields {
            modules: Option<u64>,
            taken: Option<u64>,
        }
        impl tracing::field::Visit for Fields {
            fn record_debug(&mut self, _: &tracing::field::Field, _: &dyn std::fmt::Debug) {}
            fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
                match field.name() {
                    "modules" => self.modules = Some(value),
                    "taken_from_original" => self.taken = Some(value),
                    _ => {}
                }
            }
        }
        if event.metadata().target() != "cordial::def_map" {
            return;
        }
        let mut fields = Fields::default();
        event.record(&mut fields);
        if let (Some(modules), Some(taken)) = (fields.modules, fields.taken) {
            self.0.lock().unwrap().push((modules, taken));
        }
    }
}

#[test]
fn flipping_an_atom_takes_the_modules_it_does_not_reach_from_the_map_it_made_before() {
    let _serial = SERIAL.lock().unwrap();
    let built = MapsBuilt::default();
    let subscriber = tracing_subscriber::registry().with(built.clone());
    tracing::subscriber::with_default(subscriber, || {
        let files = many(40);
        let borrowed: Vec<(&str, &str)> =
            files.iter().map(|(name, text)| (name.as_str(), text.as_str())).collect();
        let (mut db, krate) = crate_of(&borrowed, CfgOptions::default());
        let first = crate_def_map(&db, krate).modules(&db).count();
        assert_eq!(first, 41, "the root and the 40 modules");

        let mut cfg = CfgOptions::default();
        flag(&mut cfg, "a");
        krate.set_cfg_options(&mut db).to(cfg);
        assert_eq!(crate_def_map(&db, krate).modules(&db).count(), 41, "a gated function, not a module");

        let maps = built.0.lock().unwrap().clone();
        let (modules, taken) = *maps.last().unwrap();
        assert_eq!(modules, 41);
        // The module whose function is made visible is walked, and so is the root, which defines a macro: the other
        // 39 modules are taken.
        assert!(
            taken >= 39,
            "the map made again for one flipped atom took {taken} of {modules} modules from the one before: {maps:?}"
        );
    });
}

/// What a crate's definition map says, by what each module and each id stands for, so that two databases that made
/// it can be compared: each module by its path.
fn what_the_map_says(db: &RootDatabase, krate: Crate) -> std::collections::BTreeMap<String, u64> {
    let mut said: std::collections::BTreeMap<String, u64> =
        ra_ap_hir_def::nameres::stable::module_fingerprints(db, krate)
            .into_iter()
            .map(|module| (module.path, module.data))
            .collect();
    said.insert(
        "(diagnostics)".to_owned(),
        ra_ap_hir_def::nameres::stable::diagnostics_fingerprint(db, krate),
    );
    said
}

/// A crate that has what a flip can reach in every way a map can hold it.
const OF_EVERY_KIND: [(&str, &str); 15] = [
    (
        "lib.rs",
        "mod plain;\n#[cfg(feature = \"a\")]\nmod gated;\nmod user;\nmod globber;\npub mod nested;\n\
         mod macro_user;\nmod empty;\nmod globber_of_empty;\n\
         #[macro_use]\nmod mm;\nmod uses_mm;\n#[path = \"elsewhere.rs\"]\nmod renamed;\nmod traits;\nmod impls;\n\
         pub mod inline { #[cfg(feature = \"a\")] pub fn only_a() {} pub mod deeper { pub fn d() {} } pub use self::deeper::*; }\n\
         macro_rules! shout { ($e:expr) => { $e + 1 }; }\n\
         #[cfg(feature = \"b\")]\n#[macro_export]\nmacro_rules! make_items { () => { pub fn made() {} }; }\n\
         pub use plain::exported;\n#[cfg(feature = \"a\")]\npub use gated::from_gated;\npub fn root() -> u32 { shout!(1) }\n",
    ),
    ("plain.rs", "pub fn exported() {}\npub struct P;\nimpl P { pub fn go(&self) {} }\n"),
    (
        "gated.rs",
        "pub fn from_gated() {}\npub mod inner { pub fn deep() {} }\n#[cfg(feature = \"b\")]\npub fn also_b() {}\n",
    ),
    ("user.rs", "use crate::plain::P;\nuse crate::gated::inner::deep as d;\npub fn u(p: P) { p.go(); }\n"),
    ("globber.rs", "pub use crate::plain::*;\n#[cfg(feature = \"a\")]\npub use crate::gated::*;\n"),
    ("nested.rs", "pub mod one;\npub mod two { pub fn t() {} }\n#[cfg(feature = \"b\")]\npub fn nested_b() {}\n"),
    ("nested/one.rs", "pub fn o() {}\n"),
    // Calls a macro that the root defines only under `b`, by a path: it has no textual scope to inherit.
    ("macro_user.rs", "crate::make_items!();\n"),
    // Has nothing until `b`, and what imports everything from it does not change.
    ("empty.rs", "#[cfg(feature = \"b\")]\npub fn appears() {}\n"),
    ("globber_of_empty.rs", "pub use crate::empty::*;\n"),
    // A macro in a module that `#[macro_use]` makes visible to the ones after it, and a module that calls it.
    (
        "mm.rs",
        "macro_rules! from_mm { () => { pub fn from_mm() {} }; }\n#[cfg(feature = \"b\")]\nmacro_rules! only_with_b { () => { pub fn b_only() {} }; }\n",
    ),
    ("uses_mm.rs", "from_mm!();\n#[cfg(feature = \"b\")]\nonly_with_b!();\n"),
    ("elsewhere.rs", "pub fn moved() {}\n#[cfg(feature = \"a\")]\npub fn moved_a() {}\n"),
    ("traits.rs", "pub trait Speak { fn speak(&self) {} }\n#[cfg(feature = \"a\")]\npub trait OnlyA {}\n"),
    (
        "impls.rs",
        "use crate::traits::Speak;\nuse crate::plain::P;\nimpl Speak for P {}\n#[cfg(feature = \"a\")]\nimpl crate::traits::OnlyA for P {}\n",
    ),
];

#[test]
fn a_map_made_again_after_flips_is_the_map_made_from_scratch() {
    let _serial = SERIAL.lock().unwrap();
    cordial::init_tracing();
    let with = |atoms: &[&str]| {
        let mut cfg = CfgOptions::default();
        for atom in atoms {
            flag(&mut cfg, atom);
        }
        cfg
    };
    // Flips in every direction: on, more on, one off, all off, a different one.
    let sequence: [&[&str]; 6] = [&[], &["a"], &["a", "b"], &["b"], &[], &["a"]];
    let (mut flipped, krate) = crate_of(&OF_EVERY_KIND, with(sequence[0]));
    let _ = crate_def_map(&flipped, krate);
    for step in &sequence[1..] {
        krate.set_cfg_options(&mut flipped).to(with(step));
        let again = what_the_map_says(&flipped, krate);

        let (fresh_db, fresh) = crate_of(&OF_EVERY_KIND, with(step));
        let scratch = what_the_map_says(&fresh_db, fresh);
        let differing: Vec<&String> = again
            .iter()
            .filter(|(path, said)| scratch.get(*path) != Some(*said))
            .map(|(path, _)| path)
            .chain(scratch.keys().filter(|path| !again.contains_key(*path)))
            .collect();
        assert!(
            differing.is_empty(),
            "the map made again for {step:?} is not the one made from scratch: {differing:?} differ"
        );
    }
}

/// Two crates: `dep`, with `dep_cfg`, and `app`, with `app_cfg`, which depends on it as `dep`.
fn two_crates(
    dep_files: &[(&str, &str)],
    app_files: &[(&str, &str)],
    dep_cfg: CfgOptions,
    app_cfg: CfgOptions,
) -> (RootDatabase, Crate, Crate) {
    let mut db = RootDatabase::default();
    let mut graph = CrateGraphBuilder::default();
    let mut set = FileSet::default();
    let mut change = FileChange::default();
    let mut next = 0u32;
    let mut roots = Vec::new();
    for (directory, files, cfg) in
        [("dep", dep_files, dep_cfg), ("app", app_files, app_cfg)]
    {
        let first = next;
        for (name, text) in files {
            let file = FileId::from_raw(next);
            next += 1;
            set.insert(file, VfsPath::new_virtual_path(format!("/{directory}/{name}")));
            change.change_file(file, Some((*text).to_owned()));
        }
        roots.push(graph.add_crate_root(
            FileId::from_raw(first),
            Edition::CURRENT,
            None,
            None,
            cfg,
            None,
            Env::default(),
            CrateOrigin::Local { repo: None, name: None },
            Vec::new(),
            false,
            triomphe::Arc::new(AbsPathBuf::assert("/".into())),
            triomphe::Arc::new(CrateWorkspaceData {
                target: Err("fixture has no target".into()),
                toolchain: None,
            }),
        ));
    }
    graph
        .add_dep(roots[1], DependencyBuilder::new(CrateName::new("dep").unwrap(), roots[0]))
        .unwrap();
    change.set_roots(vec![SourceRoot::new_local(set)]);
    change.set_crate_graph(graph);
    let crates = change.apply(&mut db).unwrap();
    let (dep, app) = (crates[&roots[0]], crates[&roots[1]]);
    (db, dep, app)
}

/// What an app makes of a dependency whose cfg flips: it imports from it, calls its macros, and globs it.
const DEP: [(&str, &str); 2] = [
    (
        "lib.rs",
        "pub mod inner;\n#[cfg(feature = \"a\")]\npub fn gated() {}\npub fn always() {}\n\
         #[cfg(feature = \"a\")]\n#[macro_export]\nmacro_rules! from_dep { () => { pub fn from_the_macro() {} }; }\n",
    ),
    ("inner.rs", "#[cfg(feature = \"a\")]\npub struct OnlyA;\npub struct Both;\n"),
];

const APP: [(&str, &str); 4] = [
    ("lib.rs", "mod uses;\nmod globs;\nmod macros;\nmod untouched;\n"),
    ("uses.rs", "pub use dep::gated;\npub use dep::always;\npub use dep::inner::OnlyA;\npub use dep::inner::Both;\n"),
    ("globs.rs", "pub use dep::*;\n"),
    ("macros.rs", "dep::from_dep!();\n"),
];

#[test]
fn a_map_made_again_after_its_dependency_flips_is_the_map_made_from_scratch() {
    let _serial = SERIAL.lock().unwrap();
    cordial::init_tracing();
    let with = |atoms: &[&str]| {
        let mut cfg = CfgOptions::default();
        for atom in atoms {
            flag(&mut cfg, atom);
        }
        cfg
    };
    // `untouched.rs` does not exist: an unresolved module, which only the files can change.
    let (mut flipped, dep, app) = two_crates(&DEP, &APP, with(&[]), with(&[]));
    let _ = crate_def_map(&flipped, app);
    for step in [&["a"][..], &[], &["a"]] {
        dep.set_cfg_options(&mut flipped).to(with(step));
        let again = what_the_map_says(&flipped, app);
        let (fresh_db, _, fresh) = two_crates(&DEP, &APP, with(step), with(&[]));
        let scratch = what_the_map_says(&fresh_db, fresh);
        let differing: Vec<&String> = again
            .iter()
            .filter(|(path, said)| scratch.get(*path) != Some(*said))
            .map(|(path, _)| path)
            .collect();
        if !differing.is_empty() {
            let said = |db: &RootDatabase, krate: Crate| {
                let mut lines: Vec<String> =
                    crate_def_map(db, krate).diagnostics(db).iter().map(|d| format!("{:?}", d.kind)).collect();
                lines.sort();
                lines
            };
            panic!(
                "the app's map made again with the dependency at {step:?} is not the one made from scratch: \
                 {differing:?} differ\nmade again: {:#?}\nfrom scratch: {:#?}",
                said(&flipped, app),
                said(&fresh_db, fresh)
            );
        }
    }
}

#[test]
fn a_map_made_again_for_its_own_change_still_depends_on_what_the_modules_it_took_read() {
    let _serial = SERIAL.lock().unwrap();
    cordial::init_tracing();
    let with = |atoms: &[&str]| {
        let mut cfg = CfgOptions::default();
        for atom in atoms {
            flag(&mut cfg, atom);
        }
        cfg
    };
    let mut app_files: Vec<(&str, &str)> = APP.to_vec();
    app_files[0] = (
        "lib.rs",
        "mod uses;\nmod globs;\nmod macros;\nmod untouched;\n#[cfg(feature = \"x\")]\npub fn mine() {}\n",
    );
    let (mut flipped, dep, app) = two_crates(&DEP, &app_files, with(&[]), with(&[]));
    let _ = crate_def_map(&flipped, app);
    // The app's own cfg changes, so its map is made again from its own files, and `uses.rs` is taken: nothing in
    // it or in what it reads from the dependency has changed.
    app.set_cfg_options(&mut flipped).to(with(&["x"]));
    let _ = what_the_map_says(&flipped, app);
    // Then the dependency changes. The app's map must be made again, and be the one made from scratch.
    dep.set_cfg_options(&mut flipped).to(with(&["a"]));
    let again = what_the_map_says(&flipped, app);
    let (fresh_db, _, fresh) = two_crates(&DEP, &app_files, with(&["a"]), with(&["x"]));
    let scratch = what_the_map_says(&fresh_db, fresh);
    let differing: Vec<&String> =
        again.iter().filter(|(path, said)| scratch.get(*path) != Some(*said)).map(|(path, _)| path).collect();
    assert!(
        differing.is_empty(),
        "the app's map, made again for its own change and then for the dependency's, is not the one made from scratch: {differing:?} differ"
    );
}

// ---------------------------------------------------------------------------------------------------------------
// Each rule that keeps a module from being taken from the map made before, shown to fire when there is something
// for it to catch and to be quiet when there is not, on the same flip. Nothing in the source is edited to see it:
// the condition is in the fixture.
// ---------------------------------------------------------------------------------------------------------------

/// Whether the fixture has what the rule under test is there for.
#[derive(Clone, Copy, Debug)]
enum Condition {
    SomethingToCatch,
    NothingToSee,
}

/// Why the modules of each definition map were made again and not taken, as each map built said.
#[derive(Clone, Default)]
struct Reasons(Arc<Mutex<Vec<std::collections::BTreeMap<String, u64>>>>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Reasons {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        struct Fields(std::collections::BTreeMap<String, u64>);
        impl tracing::field::Visit for Fields {
            fn record_debug(&mut self, _: &tracing::field::Field, _: &dyn std::fmt::Debug) {}
            fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
                self.0.insert(field.name().to_owned(), value);
            }
        }
        if event.metadata().target() != "cordial::def_map" {
            return;
        }
        let mut fields = Fields(std::collections::BTreeMap::new());
        event.record(&mut fields);
        if fields.0.contains_key("affected_modules") {
            self.0.lock().unwrap().push(fields.0);
        }
    }
}

fn atoms(names: &[&str]) -> CfgOptions {
    let mut cfg = CfgOptions::default();
    for name in names {
        flag(&mut cfg, name);
    }
    cfg
}

/// The reasons the crate's map was made again as it was flipped from `before` to `after`, and the check that it is
/// the map made from scratch under `after`.
fn reasons_of_the_flip(
    files: &[(&str, &str)],
    before: &[&str],
    after: &[&str],
) -> std::collections::BTreeMap<String, u64> {
    let reasons = Reasons::default();
    let subscriber = tracing_subscriber::registry().with(reasons.clone());
    tracing::subscriber::with_default(subscriber, || {
        let (mut db, krate) = crate_of(files, atoms(before));
        let _ = crate_def_map(&db, krate);
        reasons.0.lock().unwrap().clear();
        krate.set_cfg_options(&mut db).to(atoms(after));
        let again = what_the_map_says(&db, krate);
        let (fresh_db, fresh) = crate_of(files, atoms(after));
        assert_eq!(
            again,
            what_the_map_says(&fresh_db, fresh),
            "the map made again for {after:?} is not the one made from scratch"
        );
    });
    let maps = reasons.0.lock().unwrap();
    maps.first().cloned().expect("the map was made again, and said why")
}

#[test]
fn a_module_with_something_unresolved_is_made_again_when_a_file_changes_and_not_otherwise() {
    let _serial = SERIAL.lock().unwrap();
    for condition in [Condition::SomethingToCatch, Condition::NothingToSee] {
        let subject = match condition {
            Condition::SomethingToCatch => "use crate::nowhere::Thing;\n",
            Condition::NothingToSee => "use crate::stable::Stable;\n",
        };
        let files = [
            ("lib.rs", "mod trigger;\nmod stable;\nmod subject;\n"),
            ("trigger.rs", "#[cfg(feature = \"a\")]\npub fn f() {}\n"),
            ("stable.rs", "pub struct Stable;\n"),
            ("subject.rs", subject),
        ];
        let reasons = reasons_of_the_flip(&files, &[], &["a"]);
        match condition {
            Condition::SomethingToCatch => assert_eq!(reasons["affected_by_unresolved"], 1, "{reasons:?}"),
            Condition::NothingToSee => {
                assert_eq!(reasons["affected_by_unresolved"], 0, "{reasons:?}");
                assert!(reasons["shared_modules"] >= 2, "`stable` and `subject` are taken: {reasons:?}");
            }
        }
    }
}

#[test]
fn a_module_that_globs_from_one_that_changes_is_made_again_even_if_the_glob_took_nothing() {
    let _serial = SERIAL.lock().unwrap();
    for condition in [Condition::SomethingToCatch, Condition::NothingToSee] {
        let subject = match condition {
            // Imports everything from a module that has nothing yet, and gets something when `a` is on.
            Condition::SomethingToCatch => "pub use crate::empty::*;\n",
            Condition::NothingToSee => "pub use crate::stable::*;\n",
        };
        let files = [
            ("lib.rs", "mod empty;\nmod stable;\nmod subject;\n"),
            ("empty.rs", "#[cfg(feature = \"a\")]\npub fn appears() {}\n"),
            ("stable.rs", "pub fn s() {}\n"),
            ("subject.rs", subject),
        ];
        let reasons = reasons_of_the_flip(&files, &[], &["a"]);
        match condition {
            Condition::SomethingToCatch => assert!(reasons["affected_by_import"] >= 1, "{reasons:?}"),
            Condition::NothingToSee => assert_eq!(reasons["affected_by_import"], 0, "{reasons:?}"),
        }
    }
}

#[test]
fn a_module_that_calls_macros_is_made_again_when_a_file_that_defines_one_changes_and_not_otherwise() {
    let _serial = SERIAL.lock().unwrap();
    for condition in [Condition::SomethingToCatch, Condition::NothingToSee] {
        // The macro is there until `a` is on, which changes the root's file; or it is always there.
        let root = match condition {
            Condition::SomethingToCatch => {
                "mod caller;\nmod trigger;\n#[cfg(not(feature = \"a\"))]\n#[macro_export]\nmacro_rules! make { () => { pub fn made() {} }; }\n"
            }
            Condition::NothingToSee => {
                "mod caller;\nmod trigger;\n#[macro_export]\nmacro_rules! make { () => { pub fn made() {} }; }\n"
            }
        };
        let files = [
            ("lib.rs", root),
            ("caller.rs", "crate::make!();\n"),
            ("trigger.rs", "#[cfg(feature = \"a\")]\npub fn f() {}\n"),
        ];
        let reasons = reasons_of_the_flip(&files, &[], &["a"]);
        match condition {
            Condition::SomethingToCatch => assert_eq!(reasons["affected_by_macro_call"], 1, "{reasons:?}"),
            Condition::NothingToSee => assert_eq!(reasons["affected_by_macro_call"], 0, "{reasons:?}"),
        }
    }
}

#[test]
fn a_module_that_imports_from_one_that_is_no_longer_declared_is_made_again() {
    let _serial = SERIAL.lock().unwrap();
    for condition in [Condition::SomethingToCatch, Condition::NothingToSee] {
        let files = [
            ("lib.rs", "#[cfg(feature = \"a\")]\nmod gated;\nmod subject;\n"),
            ("gated.rs", "pub struct G;\n"),
            ("subject.rs", "use crate::gated::G;\n"),
        ];
        // Turning `a` off takes `gated` away; turning it on only adds it.
        let (before, after): (&[&str], &[&str]) = match condition {
            Condition::SomethingToCatch => (&["a"], &[]),
            Condition::NothingToSee => (&[], &["a"]),
        };
        let reasons = reasons_of_the_flip(&files, before, after);
        match condition {
            Condition::SomethingToCatch => {
                assert!(reasons["affected_by_gone"] >= 1, "{reasons:?}");
                assert!(reasons["affected_by_import"] >= 1, "`subject` imports from it: {reasons:?}");
            }
            Condition::NothingToSee => assert_eq!(reasons["affected_by_gone"], 0, "{reasons:?}"),
        }
    }
}

#[test]
fn a_module_that_reads_another_crate_is_made_again_when_what_it_read_changes_and_not_otherwise() {
    let _serial = SERIAL.lock().unwrap();
    for condition in [Condition::SomethingToCatch, Condition::NothingToSee] {
        let dep = [("lib.rs", "#[cfg(feature = \"a\")]\npub fn gated() {}\npub fn always() {}\n")];
        let app = [
            ("lib.rs", "mod uses;\n#[cfg(feature = \"x\")]\npub fn mine() {}\n"),
            ("uses.rs", "pub use dep::always;\n"),
        ];
        // The app's own cfg changes, so its map is made again; the dependency's changes only when `a` does.
        let dependency: &[&str] = match condition {
            Condition::SomethingToCatch => &["a"],
            Condition::NothingToSee => &["unrelated"],
        };
        let reasons = Reasons::default();
        let subscriber = tracing_subscriber::registry().with(reasons.clone());
        tracing::subscriber::with_default(subscriber, || {
            let (mut db, dep_crate, app_crate) = two_crates(&dep, &app, atoms(&[]), atoms(&[]));
            let _ = crate_def_map(&db, app_crate);
            reasons.0.lock().unwrap().clear();
            dep_crate.set_cfg_options(&mut db).to(atoms(dependency));
            app_crate.set_cfg_options(&mut db).to(atoms(&["x"]));
            let again = what_the_map_says(&db, app_crate);
            let (fresh_db, _, fresh) = two_crates(&dep, &app, atoms(dependency), atoms(&["x"]));
            assert_eq!(again, what_the_map_says(&fresh_db, fresh), "{condition:?}");
        });
        let maps = reasons.0.lock().unwrap();
        let last = maps.last().cloned().expect("the app's map was made again, and said why");
        match condition {
            Condition::SomethingToCatch => assert!(last["affected_by_external"] >= 1, "{last:?}"),
            Condition::NothingToSee => {
                assert_eq!(last["affected_by_external"], 0, "{last:?}");
                assert!(last["shared_modules"] >= 1, "`uses` is taken: {last:?}");
            }
        }
    }
}

#[test]
fn a_module_that_a_flip_made_vanish_can_still_be_asked_what_it_is() {
    use ra_ap_hir_def::declaration::{Absence, Declaration, function_declaration};
    use ra_ap_hir_def::{ItemContainerId, nameres::stable};
    let _serial = SERIAL.lock().unwrap();
    let (mut db, krate) = crate_of(&GATED, atoms(&["a"]));
    let ids = function_ids(&db, krate);
    let container = |db: &RootDatabase| match ids["inside"].loc(db).container {
        ItemContainerId::ModuleId(module) => module,
        other => panic!("`inside` is declared in a module: {other:?}"),
    };
    // The module is there, and the item's id names it.
    let module = container(&db);
    assert_eq!(module.name(&db).map(|name| name.as_str().to_owned()).as_deref(), Some("proofs"));
    let _ = stable::module_fingerprints(&db, krate);

    // The atom goes off and the crate's map is made again without the module. Nothing else holds the module, and
    // what holds its id (the id of `inside`, and everything keyed by that) is not told: it is asked what it names.
    krate.set_cfg_options(&mut db).to(CfgOptions::default());
    let _ = crate_def_map(&db, krate);
    assert_eq!(module.name(&db).map(|name| name.as_str().to_owned()).as_deref(), Some("proofs"));
    assert_eq!(module.krate(&db), krate);
    // And the map says plainly that the module is not in it.
    assert_eq!(
        function_declaration(&db, ids["inside"]),
        Declaration::Absent(Absence::NotDeclared),
        "the module is not declared by the map, and it is not a module that cannot be read"
    );
}

/// A crate whose gated module has a function with a block that has an item, so that the block has a map of its own.
const GATED_BLOCK: [(&str, &str); 2] = [
    ("lib.rs", "#[cfg(feature = \"a\")]\nmod proofs;\npub fn plain() {}\n"),
    ("proofs.rs", "pub fn inside() { { struct B; unknown_macro!(); } }\n"),
];

/// What the block of a function in a module that only the flipped atom declares is asked, once the atom is off.
fn block_of_a_vanished_module() -> (RootDatabase, ra_ap_hir_def::BlockId) {
    let (mut db, krate) = crate_of(&GATED_BLOCK, atoms(&["a"]));
    let inside = function_ids(&db, krate)["inside"];
    let block = {
        let (body, _) = Body::with_source_map(&db, DefWithBodyId::FunctionId(inside));
        let blocks: Vec<_> = body.blocks(&db).map(|(block, _)| block).collect();
        assert_eq!(blocks.len(), 1, "the block that has an item has a map of its own");
        blocks[0]
    };
    krate.set_cfg_options(&mut db).to(CfgOptions::default());
    let _ = crate_def_map(&db, krate);
    (db, block)
}

#[test]
fn the_map_of_a_block_in_a_module_the_map_no_longer_has_is_not_made() {
    use ra_ap_hir_def::declaration::{Absence, Declaration, block_declaration, settle_block_map};
    let _serial = SERIAL.lock().unwrap();
    let (db, block) = block_of_a_vanished_module();
    assert_eq!(block_declaration(&db, block), Declaration::Absent(Absence::NotDeclared));
    // Settling the maps of the blocks that are held is asked of this one too, and the map of its block is not made:
    // it would look its module up in a map that does not have it.
    settle_block_map(&db, block);
}

/// Asks the crate's map for a module's data, as a query does.
#[salsa::tracked(returns(copy))]
fn asks_the_map_for_a_module(
    db: &dyn ra_ap_ide_db::base_db::SourceDatabase,
    krate: Crate,
    module: ra_ap_hir_def::ModuleId,
) -> usize {
    crate_def_map(db, krate).module_data(db, module).children.len()
}

/// Records what the `cordial::nameres` events say the queries were.
#[derive(Clone, Default)]
struct LookupFailures(Arc<Mutex<Vec<String>>>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for LookupFailures {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        struct Queries(String);
        impl tracing::field::Visit for Queries {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                if field.name() == "queries" {
                    self.0 = format!("{value:?}");
                }
            }
        }
        if event.metadata().target() == "cordial::nameres" {
            let mut queries = Queries(String::new());
            event.record(&mut queries);
            self.0.lock().unwrap().push(queries.0);
        }
    }
}

#[test]
fn a_module_the_map_does_not_have_says_which_query_asked_for_it() {
    let _serial = SERIAL.lock().unwrap();
    let (mut db, krate) = crate_of(&GATED, atoms(&["a"]));
    let module = match function_ids(&db, krate)["inside"].loc(&db).container {
        ra_ap_hir_def::ItemContainerId::ModuleId(module) => module,
        other => panic!("`inside` is declared in a module: {other:?}"),
    };
    krate.set_cfg_options(&mut db).to(CfgOptions::default());
    let _ = crate_def_map(&db, krate);

    let failures = LookupFailures::default();
    let subscriber = tracing_subscriber::registry().with(failures.clone());
    let failed = tracing::subscriber::with_default(subscriber, || {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| asks_the_map_for_a_module(&db, krate, module)))
            .is_err()
    });
    assert!(failed, "asking the map for a module it does not have is a failure");
    let said = failures.0.lock().unwrap().clone();
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(said[0].contains("asks_the_map_for_a_module"), "the query that asked is named: {said:?}");
}

/// A function whose attribute macro exists only under the atom, and which has a block with an item: the block is in
/// the file the macro call expands to.
const ATTRIBUTE_ONLY_WHEN_ON: [(&str, &str); 1] = [(
    "lib.rs",
    "#[rustc_builtin_macro]\npub macro global_allocator($item:item) {}\n#[cfg_attr(feature = \"a\", global_allocator)]\npub fn f() { { struct B; } }\n",
)];

#[test]
fn the_map_of_a_block_in_an_expansion_that_is_not_any_more_is_not_made() {
    use ra_ap_hir_def::declaration::{Absence, Declaration, block_declaration, settle_block_map};
    use salsa::plumbing::AsId;
    let _serial = SERIAL.lock().unwrap();
    let (mut db, krate) = crate_of(&ATTRIBUTE_ONLY_WHEN_ON, atoms(&["a"]));
    let f = function_ids(&db, krate)["f"];
    let block = {
        let (body, _) = Body::with_source_map(&db, DefWithBodyId::FunctionId(f));
        let blocks: Vec<_> = body.blocks(&db).map(|(block, _)| block).collect();
        assert_eq!(blocks.len(), 1, "the block that has an item has a map of its own");
        assert!(blocks[0].ast_id(&db).file_id.is_macro(), "and it is in the file the attribute macro expands to");
        blocks[0]
    };
    assert_eq!(block_declaration(&db, block), Declaration::Declared);

    // The atom is off: the attribute is not there, the function is the one in the file, and the macro call is not
    // any more. What only the flipped state had is forgotten, as the end of the flips does.
    krate.set_cfg_options(&mut db).to(CfgOptions::default());
    let _ = crate_def_map(&db, krate);
    let forgotten = <dyn salsa::Database>::forget_memos_for_inputs(&mut db, &[f.as_id()]);
    assert!(forgotten >= 1, "the body of the function of the expansion is forgotten");

    // The block was made by that body, and is gone with it, which is said before its map is asked for: it would
    // expand a call whose attribute the item does not have, and fail with `used an incorrect AttrId`.
    assert_eq!(
        block_declaration(&db, block),
        Declaration::Absent(Absence::BlockGone(salsa::Existence::Deleted))
    );
    settle_block_map(&db, block);
}

#[test]
fn the_map_of_a_block_in_an_expansion_that_is_not_any_more_fails_when_what_only_the_flip_had_is_not_forgotten() {
    use ra_ap_hir_def::declaration::{Declaration, block_declaration, settle_block_map};
    let _serial = SERIAL.lock().unwrap();
    let (mut db, krate) = crate_of(&ATTRIBUTE_ONLY_WHEN_ON, atoms(&["a"]));
    let f = function_ids(&db, krate)["f"];
    let block = {
        let (body, _) = Body::with_source_map(&db, DefWithBodyId::FunctionId(f));
        body.blocks(&db).map(|(block, _)| block).next().expect("the block has a map of its own")
    };
    krate.set_cfg_options(&mut db).to(CfgOptions::default());
    let _ = crate_def_map(&db, krate);
    // Nothing is forgotten: the block is there, its module is in the map, and so it is said to be declared. Its map is
    // the failure the amenable run had, which is what forgetting before asking prevents.
    assert_eq!(block_declaration(&db, block), Declaration::Declared);
    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| settle_block_map(&db, block))).is_err();
    assert!(failed, "without the forgetting, the stale block's map is asked for and fails");
}

/// Counts every query salsa executes, by name, from the event it says for each.
#[derive(Clone, Default)]
struct Runs(Arc<Mutex<std::collections::BTreeMap<String, usize>>>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Runs {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        struct Message(String);
        impl tracing::field::Visit for Message {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                if field.name() == "message" {
                    self.0 = format!("{value:?}");
                }
            }
        }
        if event.metadata().target() != "salsa::function::execute" {
            return;
        }
        let mut message = Message(String::new());
        event.record(&mut message);
        let name = message.0.trim_matches('"').split('(').next().unwrap_or_default().to_owned();
        if name != "new_revision" {
            *self.0.lock().unwrap().entry(name).or_default() += 1;
        }
    }
}

impl Runs {
    fn now(&self) -> std::collections::BTreeMap<String, usize> {
        self.0.lock().unwrap().clone()
    }

    /// What ran since `before`, by name.
    fn since(&self, before: &std::collections::BTreeMap<String, usize>) -> std::collections::BTreeMap<String, usize> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter_map(|(name, count)| {
                let more = count - before.get(name).copied().unwrap_or(0);
                (more > 0).then(|| (name.clone(), more))
            })
            .collect()
    }
}

/// `modules` modules, each with a macro that defines a function, a function whose body calls a macro and an item
/// that a macro call makes; the first gated by the atom `a`, and so the only module that flipping `a` changes.
fn macro_modules(modules: usize) -> Vec<(String, String)> {
    let mut files = vec![(
        "lib.rs".to_owned(),
        String::from(
            "macro_rules! define { ($name:ident) => { pub fn $name() -> u32 { 1 } }; }\n\
             macro_rules! twice { ($e:expr) => { $e + $e }; }\n",
        ) + &(0..modules).map(|m| format!("mod m{m};\n")).collect::<String>(),
    )];
    for m in 0..modules {
        let gate = if m == 0 { "#[cfg(feature = \"a\")]\n" } else { "" };
        files.push((
            format!("m{m}.rs"),
            format!(
                "{gate}define!(made_{m});\n{gate}pub fn uses_{m}() -> u32 {{ twice!({m}) }}\n\
                 pub fn plain_{m}() -> u32 {{ twice!(2) }}\n"
            ),
        ));
    }
    files
}

/// What executes in each step of what a flipped pass does, over a crate of `modules` modules: the flip and the
/// reading of what it changed, then the ordinary cfg back with what the flip changed forgotten and everything read
/// again.
fn steps_of_a_pass(modules: usize) -> [std::collections::BTreeMap<String, usize>; 2] {
    let executed = Runs::default();
    let subscriber = tracing_subscriber::registry().with(executed.clone());
    tracing::subscriber::with_default(subscriber, || {
        let files = macro_modules(modules);
        let borrowed: Vec<(&str, &str)> = files.iter().map(|(name, text)| (name.as_str(), text.as_str())).collect();
        let (mut db, krate) = crate_of(&borrowed, CfgOptions::default());
        let _ = read_bodies(&db, krate);
        read_impls(&db, krate);
        let start = salsa::plumbing::current_revision(&db);

        let before = executed.now();
        let mut flipped = CfgOptions::default();
        flag(&mut flipped, "a");
        krate.set_cfg_options(&mut db).to(flipped);
        let _ = read_bodies(&db, krate);
        read_impls(&db, krate);
        let flip = executed.since(&before);

        let before = executed.now();
        krate.set_cfg_options(&mut db).to(CfgOptions::default());
        let _ = <dyn salsa::Database>::forget_memos_changed_after(&mut db, start);
        let _ = read_bodies(&db, krate);
        read_impls(&db, krate);
        [flip, executed.since(&before)]
    })
}

#[test]
fn the_steps_of_a_flipped_pass_run_the_same_queries_in_a_crate_ten_times_the_size() {
    let _serial = SERIAL.lock().unwrap();
    let [small_flip, small_settle] = steps_of_a_pass(4);
    let [large_flip, large_settle] = steps_of_a_pass(40);
    assert_eq!(small_flip, large_flip, "what the flip executes is what the module it changes needs");
    assert_eq!(small_settle, large_settle, "what putting the ordinary cfg back executes is the same");
    // The flip makes the gated module's two macro calls run, and nothing else.
    assert_eq!(large_flip.get("MacroCallId::parse_macro_expansion_"), Some(&2), "{large_flip:?}");
    // Putting the cfg back runs the maps and item trees again and no expansion: those of the ungated modules are
    // still good, and those of the gated one are gone with it.
    assert!(
        large_settle.keys().all(|name| !name.contains("expansion") && !name.contains("macro_arg")),
        "no expansion is executed again: {large_settle:?}"
    );
}

/// What executes again when an atom that nothing reads is flipped in the crate that defines the macros `app` calls
/// `calls` times, with the maps and bodies of both read again.
fn flipped_in_the_macro_crate(calls: usize) -> std::collections::BTreeMap<String, usize> {
    let executed = Runs::default();
    let subscriber = tracing_subscriber::registry().with(executed.clone());
    tracing::subscriber::with_default(subscriber, || {
        let dep = [(
            "lib.rs",
            "#[macro_export]\nmacro_rules! twice { ($e:expr) => { $e + $e }; }\n\
             #[macro_export]\nmacro_rules! define { ($name:ident) => { pub fn $name() -> u32 { 1 } }; }\n",
        )];
        let app_text: String = (0..calls)
            .map(|n| format!("dep::define!(made_{n});\npub fn uses_{n}() -> u32 {{ dep::twice!({n}) }}\n"))
            .collect();
        let app = [("lib.rs", app_text.as_str())];
        let (mut db, dep_crate, app_crate) = two_crates(&dep, &app, CfgOptions::default(), CfgOptions::default());
        let _ = read_bodies(&db, dep_crate);
        let _ = read_bodies(&db, app_crate);
        let before = executed.now();
        let mut unrelated = CfgOptions::default();
        flag(&mut unrelated, "unrelated");
        dep_crate.set_cfg_options(&mut db).to(unrelated);
        let _ = read_bodies(&db, dep_crate);
        let _ = read_bodies(&db, app_crate);
        executed.since(&before)
    })
}

#[test]
fn flipping_an_atom_nothing_reads_in_the_crate_that_defines_a_macro_runs_none_of_the_expansions_of_its_callers() {
    let _serial = SERIAL.lock().unwrap();
    let small = flipped_in_the_macro_crate(4);
    let large = flipped_in_the_macro_crate(40);
    assert_eq!(small, large, "what is executed again does not grow with the calls of the macros: {small:?} {large:?}");
    let expansions: usize = large
        .iter()
        .filter(|(name, _)| name.contains("expansion") || name.contains("macro_arg") || name.contains("expand_"))
        .map(|(_, count)| *count)
        .sum();
    assert_eq!(expansions, 0, "no expansion of a call is executed again: {large:?}");
}

/// What executes when the bodies are read for the first time during a flip, the ordinary cfg is put back and what
/// the flip changed is forgotten, and they are read again: `modules` modules of functions that call macros, the
/// first gated by the atom `a`. Nothing about the bodies is read before the flip.
fn bodies_first_read_in_a_flip(modules: usize) -> std::collections::BTreeMap<String, usize> {
    let executed = Runs::default();
    let subscriber = tracing_subscriber::registry().with(executed.clone());
    tracing::subscriber::with_default(subscriber, || {
        let files = macro_modules(modules);
        let borrowed: Vec<(&str, &str)> = files.iter().map(|(name, text)| (name.as_str(), text.as_str())).collect();
        let (mut db, krate) = crate_of(&borrowed, CfgOptions::default());
        // What a run has computed before its contexts begin: the map, not the bodies.
        let _ = crate_def_map(&db, krate);
        let start = salsa::plumbing::current_revision(&db);

        let mut flipped = CfgOptions::default();
        flag(&mut flipped, "a");
        krate.set_cfg_options(&mut db).to(flipped);
        let _ = read_bodies(&db, krate);

        krate.set_cfg_options(&mut db).to(CfgOptions::default());
        let _ = <dyn salsa::Database>::forget_memos_changed_after(&mut db, start);
        let before = executed.now();
        let _ = read_bodies(&db, krate);
        executed.since(&before)
    })
}

#[test]
fn what_was_first_computed_during_a_flip_and_does_not_depend_on_it_is_not_forgotten_with_it() {
    let _serial = SERIAL.lock().unwrap();
    let small = bodies_first_read_in_a_flip(4);
    let large = bodies_first_read_in_a_flip(40);
    // The bodies of the modules the atom does not gate read nothing of it: what they computed while it was flipped is
    // what the ordinary cfg gives, and reading them again runs none of it. Only the gated module's are gone.
    let again = |counts: &std::collections::BTreeMap<String, usize>, name: &str| counts.get(name).copied().unwrap_or(0);
    assert_eq!(
        again(&large, "Body::with_source_map_"),
        again(&small, "Body::with_source_map_"),
        "the bodies run again do not grow with the modules that do not read the atom: {small:?} {large:?}"
    );
    assert_eq!(
        again(&large, "MacroCallId::parse_macro_expansion_"),
        again(&small, "MacroCallId::parse_macro_expansion_"),
        "nor do the expansions: {small:?} {large:?}"
    );
}

/// What executes during a flip of the atom `test`, and when the ordinary cfg is put back with what the flip changed
/// forgotten, in a crate of one module that holds a `#[cfg(test)] mod tests` and `functions` functions that call a
/// macro and name nothing of it.
fn flipped_for_a_test_module(functions: usize, one_names_it: bool) -> [std::collections::BTreeMap<String, usize>; 2] {
    let executed = Runs::default();
    let subscriber = tracing_subscriber::registry().with(executed.clone());
    tracing::subscriber::with_default(subscriber, || {
        let text: String = String::from(
            "macro_rules! twice { ($e:expr) => { $e + $e }; }\n#[cfg(test)]\nmod tests {\n    pub fn helper() {}\n    #[macro_export]\n    macro_rules! made { () => { 1 }; }\n}\n",
        ) + &(0..functions)
            .map(|n| format!("pub fn plain_{n}() -> u32 {{ twice!({n}) }}\n"))
            .collect::<String>()
            + if one_names_it { "pub fn names_it() -> u32 { crate::made!() }\n" } else { "" };
        let (mut db, krate) = crate_of(&[("lib.rs", text.as_str())], CfgOptions::default());
        let _ = read_bodies(&db, krate);
        let start = salsa::plumbing::current_revision(&db);

        let before = executed.now();
        let mut flipped = CfgOptions::default();
        flipped.insert_atom(Symbol::intern("test"));
        krate.set_cfg_options(&mut db).to(flipped);
        let _ = read_bodies(&db, krate);
        let flip = executed.since(&before);

        let before = executed.now();
        krate.set_cfg_options(&mut db).to(CfgOptions::default());
        let _ = <dyn salsa::Database>::forget_memos_changed_after(&mut db, start);
        let _ = read_bodies(&db, krate);
        [flip, executed.since(&before)]
    })
}

#[test]
fn flipping_the_test_atom_runs_none_of_the_expansions_of_functions_that_name_nothing_of_the_test_module() {
    let _serial = SERIAL.lock().unwrap();
    let [small_flip, small_settle] = flipped_for_a_test_module(4, false);
    let [large_flip, large_settle] = flipped_for_a_test_module(40, false);
    let work = |counts: &std::collections::BTreeMap<String, usize>| -> usize {
        counts
            .iter()
            .filter(|(name, _)| name.contains("expansion") || name.contains("macro_arg") || name.contains("Body::"))
            .map(|(_, count)| *count)
            .sum()
    };
    assert_eq!(work(&large_flip), work(&small_flip), "the flip: {small_flip:?} {large_flip:?}");
    assert_eq!(work(&large_settle), work(&small_settle), "putting it back: {small_settle:?} {large_settle:?}");
    assert_eq!(work(&large_settle), 0, "no body or expansion runs again: {large_settle:?}");
}

#[test]
fn flipping_the_test_atom_runs_the_function_that_names_the_test_module_and_only_that_one() {
    let _serial = SERIAL.lock().unwrap();
    let bodies = |counts: &std::collections::BTreeMap<String, usize>| {
        counts.get("Body::with_source_map_").copied().unwrap_or(0)
    };
    let [without_flip, without_settle] = flipped_for_a_test_module(40, false);
    let [with_flip, with_settle] = flipped_for_a_test_module(40, true);
    assert_eq!(bodies(&with_flip), bodies(&without_flip) + 1, "the flip: {with_flip:?} {without_flip:?}");
    assert_eq!(bodies(&with_settle), bodies(&without_settle) + 1, "putting it back: {with_settle:?} {without_settle:?}");
}

#[test]
fn the_modules_of_a_blocks_map_are_the_blocks_and_not_the_crates() {
    let _serial = SERIAL.lock().unwrap();
    let (db, krate) = crate_of(&GATED_BLOCK, atoms(&["a"]));
    let inside = function_ids(&db, krate)["inside"];
    let (body, _) = Body::with_source_map(&db, DefWithBodyId::FunctionId(inside));
    let block = body.blocks(&db).map(|(block, _)| block).next().expect("the function has a block with an item");
    let in_crate = crate_def_map(&db, krate).modules(&db).count();
    let in_block: Vec<_> = ra_ap_hir_def::nameres::block_def_map(&db, block)
        .modules(&db)
        .map(|(module, _)| module.block(&db))
        .collect();
    assert!(in_crate > 1, "the crate has modules of its own: {in_crate}");
    assert_eq!(in_block, [Some(block)], "a block's map holds the block's module and no module of the crate");
}

/// What executes during a flip of the feature `a`, and when the ordinary cfg is put back with what the flip changed
/// forgotten, in a crate whose first line is `header` and that has `functions` functions that call a macro.
fn flipped_with_a_crate_attribute(header: &str, functions: usize) -> [std::collections::BTreeMap<String, usize>; 2] {
    let executed = Runs::default();
    let subscriber = tracing_subscriber::registry().with(executed.clone());
    tracing::subscriber::with_default(subscriber, || {
        let text: String = format!("{header}\nmacro_rules! twice {{ ($e:expr) => {{ $e + $e }}; }}\n")
            + &(0..functions).map(|n| format!("pub fn plain_{n}() -> u32 {{ twice!({n}) }}\n")).collect::<String>();
        let (mut db, krate) = crate_of(&[("lib.rs", text.as_str())], CfgOptions::default());
        let _ = read_bodies(&db, krate);
        let start = salsa::plumbing::current_revision(&db);

        let before = executed.now();
        krate.set_cfg_options(&mut db).to(atoms(&["a"]));
        let _ = read_bodies(&db, krate);
        let flip = executed.since(&before);

        let before = executed.now();
        krate.set_cfg_options(&mut db).to(CfgOptions::default());
        let _ = <dyn salsa::Database>::forget_memos_changed_after(&mut db, start);
        let _ = read_bodies(&db, krate);
        [flip, executed.since(&before)]
    })
}

/// The bodies and expansions in what `flipped_with_a_crate_attribute` counted.
fn bodies_and_expansions(counts: &std::collections::BTreeMap<String, usize>) -> usize {
    counts
        .iter()
        .filter(|(name, _)| name.contains("expansion") || name.contains("macro_arg") || name.contains("Body::"))
        .map(|(_, count)| *count)
        .sum()
}

#[test]
fn flipping_an_attribute_that_turns_on_a_feature_runs_none_of_the_bodies_that_do_not_read_features() {
    let _serial = SERIAL.lock().unwrap();
    let header = "#![cfg_attr(feature = \"a\", feature(test))]";
    let [small_flip, small_settle] = flipped_with_a_crate_attribute(header, 4);
    let [large_flip, large_settle] = flipped_with_a_crate_attribute(header, 40);
    assert_eq!(bodies_and_expansions(&large_flip), 0, "the flip: {small_flip:?} {large_flip:?}");
    assert_eq!(bodies_and_expansions(&large_settle), 0, "putting it back: {small_settle:?} {large_settle:?}");
}

#[test]
fn flipping_an_attribute_that_sets_the_recursion_limit_runs_the_expansions_that_read_it_and_the_same_number_in_a_larger_crate() {
    let _serial = SERIAL.lock().unwrap();
    let header = "#![cfg_attr(feature = \"a\", recursion_limit = \"512\")]";
    let [small_flip, _] = flipped_with_a_crate_attribute(header, 4);
    let [large_flip, _] = flipped_with_a_crate_attribute(header, 40);
    // An expansion reads the limit, so each runs again: this is what the field is read for, and it must not be lost.
    assert!(bodies_and_expansions(&small_flip) > 0, "{small_flip:?}");
    assert!(bodies_and_expansions(&large_flip) > bodies_and_expansions(&small_flip), "{small_flip:?} {large_flip:?}");
}

/// What executes when what the crate `dep` has (its bodies and its impls) is read again after the map of the crate
/// `app`, which does not use `dep`, is made for the first time: `app` is a crate nothing was computed for before, as a
/// copy of a crate is.
fn dep_after_another_crates_first_map() -> std::collections::BTreeMap<String, usize> {
    let executed = Runs::default();
    let subscriber = tracing_subscriber::registry().with(executed.clone());
    tracing::subscriber::with_default(subscriber, || {
        let text: String = String::from(
            "macro_rules! twice { ($e:expr) => { $e + $e }; }\npub struct Shape;\npub trait Area { fn area(&self) -> u32; }\n",
        ) + &(0..10)
            .map(|n| {
                format!(
                    "pub fn plain_{n}() -> u32 {{ twice!({n}) }}\nimpl Shape {{ pub fn m{n}(&self) -> u32 {{ {n} }} }}\n"
                )
            })
            .collect::<String>()
            + "impl Area for Shape { fn area(&self) -> u32 { 1 } }\n";
        let (mut db, dep, app) = two_crates(
            &[("lib.rs", text.as_str())],
            &[("lib.rs", "pub fn alone() {}\n")],
            CfgOptions::default(),
            CfgOptions::default(),
        );
        let _ = read_bodies(&db, dep);
        read_impls(&db, dep);
        // What adds a copy of a crate to a database is a write, and the copy's map is made in the revision after it.
        app.set_cfg_options(&mut db).to(atoms(&["unrelated"]));
        let before = executed.now();
        let _ = crate_def_map(&db, app);
        let _ = read_bodies(&db, dep);
        read_impls(&db, dep);
        executed.since(&before)
    })
}

#[test]
fn making_the_map_of_one_crate_for_the_first_time_runs_nothing_of_another_crate_again() {
    let _serial = SERIAL.lock().unwrap();
    let ran = dep_after_another_crates_first_map();
    // The map of `app` is made, with what it needs; nothing of `dep` runs again for it.
    let of_dep: std::collections::BTreeMap<_, _> = ran
        .iter()
        .filter(|(name, _)| !matches!(name.as_str(), "crate_local_def_map" | "external_def_map" | "local_def_map" | "file_item_tree_query" | "HirFileId::ast_id_map_" | "EditionedFileId::parse_" | "real_span_map" | "cfg_atom_enabled" | "crate_module_cell"))
        .collect();
    assert!(of_dep.is_empty(), "what `dep` has is not run again for a crate it does not use: {ran:?}");
}

/// How many modules a crate has for a file, as what finds a file's modules asks: a query of the file and the map.
#[salsa::tracked(returns(copy))]
fn modules_of_a_file(
    db: &dyn ra_ap_ide_db::base_db::SourceDatabase,
    krate: Crate,
    file: FileId,
) -> usize {
    crate_def_map(db, krate).modules_for_file(db, file).count()
}

/// What executes of `modules_of_a_file` when the atom `a` adds a function to a module the crate had: the modules of the
/// file are the ones they were, and what says where they are did not change.
fn finding_the_modules_of_a_file_after_a_name_is_added() -> usize {
    let executed = Runs::default();
    let subscriber = tracing_subscriber::registry().with(executed.clone());
    tracing::subscriber::with_default(subscriber, || {
        let (mut db, krate) = crate_of(
            &[("lib.rs", "pub fn always() {}\n#[cfg(feature = \"a\")]\npub fn only_with_a() {}\n")],
            CfgOptions::default(),
        );
        let file = krate.root_file_id(&db).file_id(&db);
        let _ = modules_of_a_file(&db, krate, file);
        let before = executed.now();
        krate.set_cfg_options(&mut db).to(atoms(&["a"]));
        let _ = modules_of_a_file(&db, krate, file);
        executed.since(&before).get("modules_of_a_file").copied().unwrap_or(0)
    })
}

#[test]
fn finding_the_modules_of_a_file_does_not_run_again_when_a_name_is_added_to_a_module() {
    let _serial = SERIAL.lock().unwrap();
    assert_eq!(
        finding_the_modules_of_a_file_after_a_name_is_added(),
        0,
        "what finds the modules of a file reads where they are, not what they hold"
    );
}

/// How many traits are in scope in a module of a crate: what method resolution asks of the module a body is in.
#[salsa::tracked(returns(copy))]
fn traits_in_scope_of_the_root(db: &dyn ra_ap_ide_db::base_db::SourceDatabase, krate: Crate) -> usize {
    use ra_ap_hir_def::resolver::HasResolver as _;
    let root = crate_def_map(db, krate).root_module_id();
    root.resolver(db).traits_in_scope(db).len()
}

/// What executes of `traits_in_scope_of_the_root` when the atom `a` adds a function, and no trait, to the module.
fn traits_in_scope_after_a_function_is_added() -> usize {
    let executed = Runs::default();
    let subscriber = tracing_subscriber::registry().with(executed.clone());
    tracing::subscriber::with_default(subscriber, || {
        let (mut db, krate) = crate_of(
            &[("lib.rs", "pub trait Shape {}\npub fn always() {}\n#[cfg(feature = \"a\")]\npub fn only_with_a() {}\n")],
            CfgOptions::default(),
        );
        assert_eq!(traits_in_scope_of_the_root(&db, krate), 1);
        let before = executed.now();
        krate.set_cfg_options(&mut db).to(atoms(&["a"]));
        assert_eq!(traits_in_scope_of_the_root(&db, krate), 1);
        executed.since(&before).get("traits_in_scope_of_the_root").copied().unwrap_or(0)
    })
}

#[test]
fn the_traits_in_scope_of_a_module_do_not_run_again_when_a_function_is_added_to_it() {
    let _serial = SERIAL.lock().unwrap();
    assert_eq!(
        traits_in_scope_after_a_function_is_added(),
        0,
        "what method resolution asks of a module reads its traits, not all it holds"
    );
}

/// The same, when the atom adds a trait: the traits changed, and so it must run.
fn traits_in_scope_after_a_trait_is_added() -> usize {
    let executed = Runs::default();
    let subscriber = tracing_subscriber::registry().with(executed.clone());
    tracing::subscriber::with_default(subscriber, || {
        let (mut db, krate) = crate_of(
            &[("lib.rs", "pub trait Shape {}\n#[cfg(feature = \"a\")]\npub trait Area {}\n")],
            CfgOptions::default(),
        );
        assert_eq!(traits_in_scope_of_the_root(&db, krate), 1);
        let before = executed.now();
        krate.set_cfg_options(&mut db).to(atoms(&["a"]));
        assert_eq!(traits_in_scope_of_the_root(&db, krate), 2, "the trait the atom adds is in scope");
        executed.since(&before).get("traits_in_scope_of_the_root").copied().unwrap_or(0)
    })
}

#[test]
fn the_traits_in_scope_of_a_module_run_again_when_a_trait_is_added_to_it() {
    let _serial = SERIAL.lock().unwrap();
    assert_eq!(traits_in_scope_after_a_trait_is_added(), 1);
}

/// What executes of `modules_of_a_file` for the root file when the atom `a` adds a module in another file: the root file's
/// modules are the ones they were.
fn finding_the_modules_of_a_file_after_a_module_is_added_in_another_file() -> usize {
    let executed = Runs::default();
    let subscriber = tracing_subscriber::registry().with(executed.clone());
    tracing::subscriber::with_default(subscriber, || {
        let (mut db, krate) = crate_of(
            &[
                ("lib.rs", "pub fn always() {}\n#[cfg(feature = \"a\")]\nmod extra;\n"),
                ("extra.rs", "pub fn in_extra() {}\n"),
            ],
            CfgOptions::default(),
        );
        let file = krate.root_file_id(&db).file_id(&db);
        assert_eq!(modules_of_a_file(&db, krate, file), 1);
        let before = executed.now();
        krate.set_cfg_options(&mut db).to(atoms(&["a"]));
        assert_eq!(modules_of_a_file(&db, krate, file), 1, "the root file still holds the root module only");
        executed.since(&before).get("modules_of_a_file").copied().unwrap_or(0)
    })
}

#[test]
fn finding_the_modules_of_a_file_does_not_run_again_when_a_module_is_added_in_another_file() {
    let _serial = SERIAL.lock().unwrap();
    assert_eq!(
        finding_the_modules_of_a_file_after_a_module_is_added_in_another_file(),
        0,
        "what finds the modules of one file does not depend on the modules of the others"
    );
}

/// The inference of every function of the crate.
fn infer_bodies(db: &RootDatabase, krate: Crate) {
    let functions: Vec<_> = crate_def_map(db, krate)
        .modules(db)
        .flat_map(|(_, module)| module.scope.declarations().collect::<Vec<_>>())
        .filter_map(|def| match def {
            ModuleDefId::FunctionId(id) => Some(DefWithBodyId::from(id)),
            _ => None,
        })
        .collect();
    for function in functions {
        let _ = ra_ap_hir_ty::InferenceResult::of(db, function);
    }
}

/// What an eviction between two reads can drop, as `evict_regenerable` does after each context.
#[derive(Clone, Copy)]
enum Dropped {
    Nothing,
    TheLru,
    TheScaffolding,
}

/// The queries the eviction of a run lets go of as scaffolding (`SCAFFOLDING` in `src/loader/database/eviction.rs`).
const SCAFFOLDING: [&str; 5] = [
    "EditionedFileId::parse_",
    "MacroCallId::parse_macro_expansion_",
    "HirFileId::ast_id_map_",
    "MacroCallId::macro_arg_",
    "real_span_map",
];

/// How many values of the queries `only` the database holds.
fn held(db: &mut RootDatabase, only: &[&str]) -> u64 {
    let mut held = 0;
    <dyn salsa::Database>::values_in_memory(db, false, true, only, &mut |_, n| held += n);
    held
}

/// What executes when the crate of `files` is flipped to `flipped` after the bodies and their inference were read,
/// and the inference is read again; with `dropped` dropped between the reads.
fn executed_by_flipping_after(
    dropped: Dropped,
    files: &[(String, String)],
    flipped: &CfgOptions,
) -> std::collections::BTreeMap<String, usize> {
    let executed = Runs::default();
    let subscriber = tracing_subscriber::registry().with(executed.clone());
    tracing::subscriber::with_default(subscriber, || {
        let borrowed: Vec<(&str, &str)> = files.iter().map(|(name, text)| (name.as_str(), text.as_str())).collect();
        let (mut db, krate) = crate_of(&borrowed, CfgOptions::default());
        infer_bodies(&db, krate);
        let (queries, before): (&[&str], u64) = match dropped {
            Dropped::Nothing => (&[], 0),
            Dropped::TheLru => (&["Body::with_source_map_"], held(&mut db, &["Body::with_source_map_"])),
            Dropped::TheScaffolding => (&SCAFFOLDING, held(&mut db, &SCAFFOLDING)),
        };
        match dropped {
            Dropped::Nothing => {}
            Dropped::TheLru => {
                salsa::cap_lru_capacities(1);
                <dyn salsa::Database>::trigger_lru_eviction(&mut db);
                salsa::cap_lru_capacities(0);
            }
            Dropped::TheScaffolding => {
                <dyn salsa::Database>::values_in_memory(&mut db, true, false, &SCAFFOLDING, &mut |_, _| {});
            }
        }
        if !queries.is_empty() {
            let after = held(&mut db, queries);
            assert!(after < before, "nothing was dropped, so this shows nothing: {before} held, {after} held after");
        }
        krate.set_cfg_options(&mut db).to(flipped.clone());
        let before = executed.now();
        infer_bodies(&db, krate);
        executed.since(&before)
    })
}

/// What executes when the atom `a`, which only the first of `modules` modules tests, is flipped.
fn executed_by_a_flip_after(dropped: Dropped, modules: usize) -> std::collections::BTreeMap<String, usize> {
    let mut flipped = CfgOptions::default();
    flag(&mut flipped, "a");
    executed_by_flipping_after(dropped, &macro_modules(modules), &flipped)
}

/// What executes when the recursion limit, which every expansion reads, is changed by a flip.
fn executed_by_a_limit_flip_after(dropped: Dropped, functions: usize) -> std::collections::BTreeMap<String, usize> {
    let text = String::from(
        "#![cfg_attr(feature = \"a\", recursion_limit = \"512\")]\nmacro_rules! twice { ($e:expr) => { $e + $e }; }\n",
    ) + &(0..functions).map(|n| format!("pub fn plain_{n}() -> u32 {{ twice!({n}) }}\n")).collect::<String>();
    let mut flipped = CfgOptions::default();
    flag(&mut flipped, "a");
    executed_by_flipping_after(dropped, &[("lib.rs".to_owned(), text)], &flipped)
}

/// A body that was dropped has no value to be compared with when it runs again, so what read it directly runs once
/// more (`Body::of_`, which returns it as it was); what read that does not, because that comes out the same.
#[test]
fn dropping_the_bodies_makes_only_the_query_that_reads_them_run_when_they_come_out_the_same() {
    let _serial = SERIAL.lock().unwrap();
    let kept = executed_by_a_limit_flip_after(Dropped::Nothing, 8);
    let dropped = executed_by_a_limit_flip_after(Dropped::TheLru, 8);
    let work = |counts: &std::collections::BTreeMap<String, usize>, name: &str| counts.get(name).copied().unwrap_or(0);
    // The bodies read the limit, so each runs again, and comes out as it was: what read it does not run.
    assert_eq!(work(&kept, "Body::with_source_map_"), 8, "every body reads the limit: {kept:?}");
    assert_eq!(the_inference_that_ran(&kept), 0, "and the same body stops what read it: {kept:?}");
    assert_eq!(work(&kept, "Body::of_"), 0, "{kept:?}");
    // Seven of the eight were dropped (the LRU keeps the last one it saw): each makes `Body::of_` run, and no more.
    assert_eq!(work(&dropped, "Body::of_"), 7, "kept {kept:?} dropped {dropped:?}");
    assert_eq!(the_inference_that_ran(&dropped), 0, "kept {kept:?} dropped {dropped:?}");
}

/// What ran, without the scaffolding, which is made again when it is read after it was dropped, by design.
fn without_scaffolding(counts: std::collections::BTreeMap<String, usize>) -> std::collections::BTreeMap<String, usize> {
    counts.into_iter().filter(|(name, _)| !SCAFFOLDING.contains(&name.as_str())).collect()
}

fn the_inference_that_ran(counts: &std::collections::BTreeMap<String, usize>) -> usize {
    counts.get("InferenceResult < 'db >::for_body_").copied().unwrap_or(0)
}

#[test]
fn dropping_the_values_the_lru_keeps_does_not_make_what_a_flip_does_not_reach_run_again() {
    let _serial = SERIAL.lock().unwrap();
    let kept = executed_by_a_flip_after(Dropped::Nothing, 8);
    let dropped = executed_by_a_flip_after(Dropped::TheLru, 8);
    assert!(the_inference_that_ran(&kept) > 0, "the flip reaches the first module: {kept:?}");
    assert_eq!(without_scaffolding(dropped), without_scaffolding(kept), "bodies that were dropped have the value they had");
}

#[test]
fn dropping_the_scaffolding_does_not_make_what_a_flip_does_not_reach_run_again() {
    let _serial = SERIAL.lock().unwrap();
    let kept = executed_by_a_flip_after(Dropped::Nothing, 8);
    let dropped = executed_by_a_flip_after(Dropped::TheScaffolding, 8);
    assert_eq!(without_scaffolding(dropped), without_scaffolding(kept), "trees, expansions and arguments that were dropped have the value they had");
}

/// The id map of every file of a crate that a module declares.
fn read_id_maps(db: &RootDatabase, krate: Crate) {
    let files: Vec<_> = crate_def_map(db, krate)
        .modules(db)
        .filter_map(|(_, module)| module.origin.file_id())
        .collect();
    for file in files {
        let _ = ra_ap_hir_expand::HirFileId::from(file).ast_id_map(db);
    }
}

/// How many id maps are made again when they are all read again after an input is written, as a flip writes one, in
/// a crate of more files than the LRU of id maps keeps (1,024), with the policy of a run or without it, and with the run short of memory or not.
fn id_maps_made_again(policy_of_a_run: bool, pressed: bool) -> usize {
    let executed = Runs::default();
    let subscriber = tracing_subscriber::registry().with(executed.clone());
    tracing::subscriber::with_default(subscriber, || {
        if policy_of_a_run {
            cordial::hold_lru_values_unless_pressed(false);
        }
        if pressed {
            cordial::feel_pressure(true);
        }
        let files: Vec<(String, String)> = std::iter::once((
            "lib.rs".to_owned(),
            (0..1100).map(|n| format!("mod m{n};\n")).collect::<String>(),
        ))
        .chain((0..1100).map(|n| (format!("m{n}.rs"), "pub fn f() {}\n".to_owned())))
        .collect();
        let borrowed: Vec<(&str, &str)> = files.iter().map(|(name, text)| (name.as_str(), text.as_str())).collect();
        let (mut db, krate) = crate_of(&borrowed, CfgOptions::default());
        read_id_maps(&db, krate);
        krate.set_cfg_options(&mut db).to(atoms(&["a"]));
        let before = executed.now();
        read_id_maps(&db, krate);
        executed.since(&before).get("HirFileId::ast_id_map_").copied().unwrap_or(0)
    })
}

#[test]
fn a_write_does_not_drop_the_id_maps_of_a_run_that_is_not_short_of_memory() {
    let _serial = SERIAL.lock().unwrap();
    // The control: with the marks the editor uses, what is over the 1,024 is dropped by the write.
    assert!(id_maps_made_again(false, false) > 0, "the marks drop id maps when an input is written");
    assert_eq!(id_maps_made_again(true, false), 0, "a run keeps them while it has the memory");
}

#[test]
fn a_write_drops_the_id_maps_over_the_marks_of_a_run_that_is_short_of_memory() {
    let _serial = SERIAL.lock().unwrap();
    assert!(id_maps_made_again(true, true) > 0, "pressure puts the marks back, and the memory is given back");
}

/// What executes of the query that finds the modules of a file, when every file's modules are read again after the atom
/// `a` adds a module in a file of its own, in a crate of `files` files that the atom does not reach.
fn finding_the_modules_of_every_file_after_a_module_is_added(files: usize) -> std::collections::BTreeMap<String, usize> {
    let executed = Runs::default();
    let subscriber = tracing_subscriber::registry().with(executed.clone());
    tracing::subscriber::with_default(subscriber, || {
        let mut sources: Vec<(String, String)> = vec![(
            "lib.rs".to_owned(),
            (0..files).map(|n| format!("mod f{n};\n")).collect::<String>() + "#[cfg(feature = \"a\")]\nmod extra;\n",
        )];
        sources.extend((0..files).map(|n| (format!("f{n}.rs"), "pub fn x() {}\n".to_owned())));
        sources.push(("extra.rs".to_owned(), "pub fn y() {}\n".to_owned()));
        let borrowed: Vec<(&str, &str)> = sources.iter().map(|(name, text)| (name.as_str(), text.as_str())).collect();
        let (mut db, krate) = crate_of(&borrowed, CfgOptions::default());
        let every_file = |db: &RootDatabase| -> usize {
            (0..sources.len())
                .map(|id| modules_of_a_file(db, krate, FileId::from_raw(u32::try_from(id).unwrap())))
                .sum()
        };
        let _ = every_file(&db);
        let before = executed.now();
        krate.set_cfg_options(&mut db).to(atoms(&["a"]));
        let _ = every_file(&db);
        executed.since(&before)
    })
}

#[test]
fn finding_the_modules_of_a_file_does_not_run_for_every_file_when_a_module_is_added_in_one() {
    let _serial = SERIAL.lock().unwrap();
    let ran = |counts: &std::collections::BTreeMap<String, usize>| counts.get("crate_modules_in_file").copied().unwrap_or(0);
    let small = finding_the_modules_of_every_file_after_a_module_is_added(4);
    let large = finding_the_modules_of_every_file_after_a_module_is_added(40);
    assert_eq!(ran(&large), ran(&small), "the lists of the files the atom does not reach: {small:?} {large:?}");
}

#[test]
fn the_modules_of_a_file_are_still_found_when_a_flip_adds_a_module_to_it_and_when_it_takes_it_away() {
    let _serial = SERIAL.lock().unwrap();
    let sources = [
        ("lib.rs", "mod f0;\n#[cfg(feature = \"a\")]\nmod extra;\n"),
        ("f0.rs", "pub fn x() {}\n"),
        ("extra.rs", "pub fn y() {}\n"),
    ];
    let (mut db, krate) = crate_of(&sources, CfgOptions::default());
    let (lib, f0, extra) = (FileId::from_raw(0), FileId::from_raw(1), FileId::from_raw(2));
    let modules = |db: &RootDatabase| [lib, f0, extra].map(|file| modules_of_a_file(db, krate, file));
    assert_eq!(modules(&db), [1, 1, 0], "before: `extra` is not declared");
    krate.set_cfg_options(&mut db).to(atoms(&["a"]));
    assert_eq!(modules(&db), [1, 1, 1], "the flip adds the module of `extra.rs`, and the others are as they were");
    krate.set_cfg_options(&mut db).to(CfgOptions::default());
    assert_eq!(modules(&db), [1, 1, 0], "and putting it back takes it away");
}

#[test]
fn the_counters_say_what_started_a_revision_what_was_written_and_what_a_walk_found() {
    let _serial = SERIAL.lock().unwrap();
    let files = macro_modules(4);
    let borrowed: Vec<(&str, &str)> = files.iter().map(|(name, text)| (name.as_str(), text.as_str())).collect();
    let (mut db, krate) = crate_of(&borrowed, CfgOptions::default());
    infer_bodies(&db, krate);
    let start = salsa::plumbing::current_revision(&db);
    let (revisions, writes, memos, outcomes) = (
        salsa::revisions_started(),
        salsa::input_writes().get("Crate").copied().unwrap_or(0),
        salsa::deep_verifications().0,
        salsa::deep_verification_outcomes(),
    );

    // A write is a revision, and says which input it wrote.
    krate.set_cfg_options(&mut db).to(atoms(&["a"]));
    assert_eq!(salsa::revisions_started() - revisions, 1, "a write starts a revision");
    assert_eq!(salsa::input_writes().get("Crate").copied().unwrap_or(0) - writes, 1, "and is a write of the crate");

    // What is read after it is walked, and every walk finds the memo as it was or finds a change.
    infer_bodies(&db, krate);
    let walked = salsa::deep_verifications().0 - memos;
    let (unchanged, changed) = (
        salsa::deep_verification_outcomes().0 - outcomes.0,
        salsa::deep_verification_outcomes().1 - outcomes.1,
    );
    assert!(walked > 0 && unchanged > 0, "the bodies are walked and most are as they were: {walked} {unchanged}");
    assert_eq!(unchanged + changed, walked, "each walk finds one or the other");
    assert!(
        salsa::deep_unchanged_by_ingredient().get("Body::of_").copied().unwrap_or(0) > 0,
        "and the walks that found nothing are said by query"
    );

    // A forget is a revision too.
    let before = salsa::revisions_started();
    let forgotten = <dyn salsa::Database>::forget_memos_changed_after(&mut db, start);
    assert!(forgotten > 0, "the flip made something");
    assert_eq!(salsa::revisions_started() - before, 1, "forgetting starts a revision");
}

/// What a segment said: its label, how many revisions started in it, how many inputs were written, and which.
type SaidOfRevisions = (String, u64, u64, String);

#[derive(Clone, Default)]
struct SegmentSaid(Arc<Mutex<Vec<SaidOfRevisions>>>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for SegmentSaid {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        #[derive(Default)]
        struct Fields {
            label: String,
            writes_top: String,
            revisions: u64,
            writes: u64,
        }
        impl tracing::field::Visit for Fields {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                match field.name() {
                    "label" => self.label = format!("{value:?}"),
                    "writes_top" => self.writes_top = format!("{value:?}").trim_matches('"').to_owned(),
                    _ => {}
                }
            }
            fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
                match field.name() {
                    "revisions" => self.revisions = value,
                    "writes" => self.writes = value,
                    _ => {}
                }
            }
        }
        if event.metadata().target() == "cordial::segment" {
            let mut fields = Fields::default();
            event.record(&mut fields);
            self.0.lock().unwrap().push((fields.label, fields.revisions, fields.writes, fields.writes_top));
        }
    }
}

#[test]
fn a_region_says_how_many_revisions_started_in_it_and_which_inputs_were_written() {
    let _serial = SERIAL.lock().unwrap();
    let said = SegmentSaid::default();
    let subscriber = tracing_subscriber::registry().with(said.clone());
    tracing::subscriber::with_default(subscriber, || {
        let (mut db, krate) = crate_of(&[("lib.rs", "pub fn f() {}\n")], CfgOptions::default());
        {
            let _quiet = cordial::Segment::begin("quiet");
            infer_bodies(&db, krate);
        }
        {
            let _outer = cordial::Segment::begin("outer");
            let _written = cordial::Segment::begin("written");
            krate.set_cfg_options(&mut db).to(atoms(&["a"]));
            krate.set_cfg_options(&mut db).to(atoms(&["b"]));
        }
    });
    let said = said.0.lock().unwrap().clone();
    let of = |label: &str| said.iter().find(|(said, ..)| said == label).cloned().unwrap();
    assert_eq!((of("quiet").1, of("quiet").2), (0, 0), "a region that writes nothing starts no revision: {said:?}");
    assert_eq!((of("outer/written").1, of("outer/written").2), (2, 2), "two writes are two revisions: {said:?}");
    assert_eq!(of("outer/written").3, "Crate=2", "and the input is named: {said:?}");
    assert_eq!(of("outer").1, 2, "a region holds what the ones in it did: {said:?}");
}

/// What a query that is running has read so far, said from inside it.
#[salsa::tracked(returns(clone))]
fn what_a_query_read(db: &dyn ra_ap_ide_db::base_db::SourceDatabase, krate: Crate) -> Vec<(String, u64)> {
    let _ = crate_def_map(db, krate);
    let in_order: Vec<String> =
        salsa::current_query_inputs_in_order().into_iter().map(|(name, _)| name.to_owned()).collect();
    assert_eq!(in_order.first().map(String::as_str), Some("external_def_map"), "it read the map first: {in_order:?}");
    salsa::current_query_inputs().into_iter().map(|(name, count)| (name.to_owned(), count)).collect()
}

#[test]
fn a_query_can_say_what_it_has_read_so_far() {
    let _serial = SERIAL.lock().unwrap();
    let (db, krate) = crate_of(&[("lib.rs", "pub fn f() {}\n")], CfgOptions::default());
    let read = what_a_query_read(&db, krate);
    assert!(
        read.iter().any(|(name, count)| name == "external_def_map" && *count >= 1),
        "it read the definition map of the crate, which it says by name: {read:?}"
    );
    assert!(salsa::current_query_inputs().is_empty(), "outside any query there is nothing it has read");
}

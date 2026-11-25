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

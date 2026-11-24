//! What toggling one cfg atom of a crate in place re-executes.
//!
//! A context could be the same crate with some atoms flipped, if what rust-analyzer computes named
//! the atoms it tested: flip one and only what tested it would run again, and what ran again to the
//! same answer would stop the change there. These tests flip an atom of one crate in place and count
//! the item trees that execute again.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use ra_ap_cfg::CfgOptions;
use ra_ap_hir_def::nameres::crate_def_map;
use ra_ap_hir_expand::intern::Symbol;
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
        .modules()
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

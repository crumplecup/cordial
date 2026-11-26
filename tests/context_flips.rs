//! A cfg context analysed by flipping the ordinary crates' own cfg gives what the copies gave.
//!
//! The same expectations as `cfg_contexts.rs`, read through `CfgContextMode::Flips`: no crate is added, the cfg of
//! the workspace's own crates is written, the context is read from the same crates and files, and the ordinary cfg
//! is written back.

use cordial::{
    CfgContextMode, CfgPlanning, CrateIr, CrateTarget, EdgeKind, IrView, NodeId,
    SemanticCargoOptions, SemanticContextKey, SourceDatabase,
};
use miette::{IntoDiagnostic, Result};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tracing_subscriber::layer::SubscriberExt;

static SERIAL: Mutex<()> = Mutex::new(());

const LIBRARY: &str = "\
pub fn helper() -> Result<(), std::io::Error> { Ok(()) }

#[cfg(kani)]
pub fn harness() { let _ = helper(); }

#[cfg(creusot)]
pub fn creusot_harness() { let _ = helper(); }

#[cfg(not(kani))]
pub fn ordinary_only() {}

#[cfg(kani)]
pub fn exclusive() {}

#[cfg(creusot)]
pub fn exclusive() {}
";


fn kani() -> SemanticContextKey {
    SemanticContextKey::enabling(["kani"])
}

fn creusot() -> SemanticContextKey {
    SemanticContextKey::enabling(["creusot"])
}

fn fixture() -> Result<tempfile::TempDir> {
    fixture_with(LIBRARY)
}

fn fixture_with(library: &str) -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir_all(root.path().join("src")).into_diagnostic()?;
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='gated'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    std::fs::write(root.path().join("src/lib.rs"), library).into_diagnostic()?;
    Ok(root)
}

/// Counts the writes of a context's cfg into the ordinary crates, and the crates a context copied.
#[derive(Clone, Default)]
struct Flipped {
    written: Arc<AtomicUsize>,
    copied: Arc<AtomicUsize>,
    /// How many functions a context kept full facts for, per context.
    kept: Arc<Mutex<Vec<u64>>>,
    /// The audit of each context: modules, changed, added, removed, crates changed.
    audit: Arc<Mutex<Vec<[u64; 5]>>>,
}

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Flipped {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _: tracing_subscriber::layer::Context<'_, S>,
    ) {
        #[derive(Default)]
        struct Audit([Option<u64>; 5]);
        impl tracing::field::Visit for Audit {
            fn record_debug(&mut self, _: &tracing::field::Field, _: &dyn std::fmt::Debug) {}
            fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
                let at = ["modules", "modules_changed", "modules_added", "modules_removed", "crates_changed"]
                    .iter()
                    .position(|name| *name == field.name());
                if let Some(at) = at {
                    self.0[at] = Some(value);
                }
            }
        }
        let mut audit = Audit::default();
        event.record(&mut audit);
        if let [Some(a), Some(b), Some(c), Some(d), Some(e)] = audit.0 {
            self.audit.lock().unwrap().push([a, b, c, d, e]);
        }
        struct Message(String, Option<u64>);
        impl tracing::field::Visit for Message {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                if field.name() == "message" {
                    self.0 = format!("{value:?}");
                }
            }
            fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
                if field.name() == "kept" {
                    self.1 = Some(value);
                }
            }
        }
        let mut message = Message(String::new(), None);
        event.record(&mut message);
        if let Some(kept) = message.1 {
            self.kept.lock().unwrap().push(kept);
        }
        if message.0.contains("cfg is written into the ordinary crates") {
            self.written.fetch_add(1, Ordering::SeqCst);
        }
        if message.0.contains("the files this context copies") {
            self.copied.fetch_add(1, Ordering::SeqCst);
        }
    }
}

/// Only the named contexts, with no planning, flipped in place.
fn project(contexts: Vec<SemanticContextKey>, root: &Path, flipped: &Flipped) -> Result<CrateIr> {
    let options = SemanticCargoOptions::default()
        .with_cfg_contexts(contexts)
        .with_cfg_planning(CfgPlanning::Explicit)
        .with_cfg_context_mode(CfgContextMode::Flips);
    let subscriber = tracing_subscriber::registry().with(flipped.clone());
    tracing::subscriber::with_default(subscriber, || {
        let mut session = SourceDatabase::load_with(root, &options).into_diagnostic()?;
        session
            .project(&CrateTarget::new("gated", root), &[root.join("src/lib.rs")])
            .into_diagnostic()
    })
}

fn functions(graph: &CrateIr, name: &str) -> Vec<NodeId> {
    let mut found: Vec<_> = graph
        .nodes_matching(&cordial::BasicQuery::items())
        .into_iter()
        .filter(|node| node.weight().name().as_deref() == Some(name))
        .map(|node| node.id())
        .collect();
    found.sort_by_key(|id| id.value());
    found
}

fn only(graph: &CrateIr, name: &str) -> Result<NodeId> {
    match functions(graph, name).as_slice() {
        [id] => Ok(*id),
        other => Err(miette::miette!("{name}: {other:?}")),
    }
}

fn active(graph: &CrateIr, id: NodeId) -> Result<Vec<SemanticContextKey>> {
    let mut contexts = graph
        .active_contexts(id)
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("no semantic projection"))?;
    contexts.sort();
    Ok(contexts)
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_context_flipped_in_place_resolves_gated_code_and_its_calls() -> Result<()> {
    let _serial = SERIAL.lock().unwrap();
    cordial::init_tracing();
    let root = fixture()?;
    let flipped = Flipped::default();
    let graph = project(vec![kani()], root.path(), &flipped)?;
    assert_eq!(flipped.written.load(Ordering::SeqCst), 1, "the cfg is written, once, for the one context");
    assert_eq!(flipped.copied.load(Ordering::SeqCst), 0, "and no crate or file is copied");
    let (helper, harness) = (only(&graph, "helper")?, only(&graph, "harness")?);

    assert_eq!(active(&graph, harness)?, vec![kani()]);
    assert!(
        graph.semantic_function(harness).into_diagnostic()?.is_none(),
        "gated code has no ordinary facts"
    );
    let entries = graph.semantic_context_functions(harness).into_diagnostic()?;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].context(), &kani());
    assert_eq!(entries[0].function().calls().len(), 1);

    assert_eq!(graph.context_children(harness, &kani()), vec![helper]);
    assert_eq!(graph.context_parents(helper, &kani()), vec![harness]);
    assert!(graph.children(harness, EdgeKind::Calls).is_empty());
    assert!(graph.parents(helper, EdgeKind::Calls).is_empty());
    assert!(graph.context_parents(helper, &creusot()).is_empty());

    // Code the context turns off is no longer active there, and the ordinary build is as it was.
    assert_eq!(
        active(&graph, only(&graph, "ordinary_only")?)?,
        vec![SemanticContextKey::ordinary()]
    );
    let mut helper_contexts = vec![SemanticContextKey::ordinary(), kani()];
    helper_contexts.sort();
    assert_eq!(active(&graph, helper)?, helper_contexts);
    Ok(())
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn exclusive_verifier_cfgs_flipped_one_after_the_other_never_see_both() -> Result<()> {
    let _serial = SERIAL.lock().unwrap();
    cordial::init_tracing();
    let root = fixture()?;
    let flipped = Flipped::default();
    let graph = project(vec![kani(), creusot()], root.path(), &flipped)?;
    assert_eq!(flipped.written.load(Ordering::SeqCst), 2);
    let helper = only(&graph, "helper")?;

    let exclusive = functions(&graph, "exclusive");
    assert_eq!(exclusive.len(), 2, "{exclusive:?}");
    let mut seen = Vec::new();
    for id in exclusive {
        seen.push(active(&graph, id)?);
    }
    seen.sort();
    assert_eq!(seen, vec![vec![creusot()], vec![kani()]]);

    assert_eq!(graph.context_children(only(&graph, "harness")?, &kani()), vec![helper]);
    assert_eq!(
        graph.context_children(only(&graph, "creusot_harness")?, &creusot()),
        vec![helper]
    );
    // `ordinary_only` is `not(kani)`: off in the kani context, on in the creusot one, and the cfg that was
    // written for kani is not still there for creusot.
    let mut ordinary_only = vec![SemanticContextKey::ordinary(), creusot()];
    ordinary_only.sort();
    assert_eq!(active(&graph, only(&graph, "ordinary_only")?)?, ordinary_only);
    Ok(())
}

/// A function that spells a name the gate defines, and does not use what it defines.
const NAMES_A_FLIPPED_NAME: &str = "\
pub fn helper() {}

#[cfg(kani)]
pub fn harness() { helper(); }

pub fn spells_it() -> u32 { let harness = 1; harness + 1 }

pub fn alone() -> u32 { 2 }
";

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_function_keeps_a_contexts_facts_only_when_they_differ_from_the_ordinary_ones() -> Result<()> {
    let _serial = SERIAL.lock().unwrap();
    cordial::init_tracing();
    let root = fixture_with(NAMES_A_FLIPPED_NAME)?;
    let flipped = Flipped::default();
    let graph = project(vec![kani()], root.path(), &flipped)?;
    // Only `harness`, which the context makes exist: `spells_it` has the word but not the dependence, and
    // `helper` and `alone` read nothing the gate changes.
    assert_eq!(*flipped.kept.lock().unwrap(), vec![1]);
    for name in ["helper", "spells_it", "alone"] {
        assert!(
            graph
                .semantic_context_functions(only(&graph, name)?)
                .into_diagnostic()?
                .is_empty(),
            "{name}: its facts in the context are the ordinary ones, and are not kept again"
        );
    }
    Ok(())
}

/// A crate of a root and six modules, one of which has a gated function.
fn many_modules() -> Result<tempfile::TempDir> {
    let root = fixture_with(
        &(0..6).map(|m| format!("mod m{m};\n")).collect::<String>(),
    )?;
    for m in 0..6 {
        let gate = if m == 0 { "#[cfg(kani)]\npub fn gated() {}\n" } else { "" };
        std::fs::write(
            root.path().join(format!("src/m{m}.rs")),
            format!("{gate}pub fn f{m}() -> u32 {{ {m} }}\npub struct S{m};\nimpl S{m} {{ pub fn get(&self) -> u32 {{ {m} }} }}\n"),
        )
        .into_diagnostic()?;
    }
    Ok(root)
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_flip_says_how_many_modules_of_the_crate_it_changed() -> Result<()> {
    let _serial = SERIAL.lock().unwrap();
    cordial::init_tracing();
    let root = many_modules()?;
    let flipped = Flipped::default();
    let _graph = project(vec![kani()], root.path(), &flipped)?;
    // Seven modules; the one with the gate has one declaration more, and nothing else differs.
    assert_eq!(*flipped.audit.lock().unwrap(), vec![[7, 1, 0, 0, 1]]);
    Ok(())
}

const A_GATED_MODULE: &str = "\
pub fn ordinary() -> u32 { 1 }

#[cfg(kani)]
pub mod proofs {
    pub fn harness() -> u32 { crate::ordinary() }
    pub struct Witness(pub u32);
    impl Witness { pub fn get(&self) -> u32 { self.0 } }
}
";

/// One run of a flipped kani context that saves its state, and takes up the state a run before it saved.
fn run_saving(root: &Path, saved: &cordial::SavedState) -> Result<(bool, CrateIr)> {
    let options = SemanticCargoOptions::default()
        .with_cfg_contexts(vec![kani()])
        .with_cfg_planning(CfgPlanning::Explicit)
        .with_cfg_context_mode(CfgContextMode::Flips);
    let mut session =
        SourceDatabase::load_saved(root, &options, Some(saved.clone())).into_diagnostic()?;
    let restored = session.restored();
    let graph = session
        .project(&CrateTarget::new("gated", root), &[root.join("src/lib.rs")])
        .into_diagnostic()?;
    Ok((restored, graph))
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_module_only_a_flipped_context_has_does_not_stop_the_state_from_being_saved() -> Result<()> {
    let _serial = SERIAL.lock().unwrap();
    cordial::init_tracing();
    let root = fixture_with(A_GATED_MODULE)?;
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = cordial::SavedState::at(state.path().join("analysis-state"));

    let (restored, cold) = run_saving(root.path(), &saved)?;
    assert!(!restored);
    assert_eq!(active(&cold, only(&cold, "harness")?)?, vec![kani()]);

    // The next run starts from what the first saved, and finds the same.
    let (restored, warm) = run_saving(root.path(), &saved)?;
    assert!(restored, "the second run starts from the first's state");
    assert_eq!(active(&warm, only(&warm, "harness")?)?, vec![kani()]);
    assert_eq!(active(&warm, only(&warm, "ordinary")?)?.len(), 2);
    Ok(())
}

const A_GATED_DERIVE: &str = "\
pub fn ordinary() -> u32 { 1 }

#[cfg_attr(kani, derive(Clone))]
pub struct Witness(pub u32);

#[cfg(kani)]
pub fn harness() -> u32 { Witness(1).clone().0 }
";

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_derive_only_a_flipped_context_has_does_not_stop_the_state_from_being_saved() -> Result<()> {
    let _serial = SERIAL.lock().unwrap();
    cordial::init_tracing();
    let root = fixture_with(A_GATED_DERIVE)?;
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = cordial::SavedState::at(state.path().join("analysis-state"));

    let (restored, cold) = run_saving(root.path(), &saved)?;
    assert!(!restored);
    assert_eq!(active(&cold, only(&cold, "harness")?)?, vec![kani()]);
    let (restored, warm) = run_saving(root.path(), &saved)?;
    assert!(restored, "the second run starts from the first's state");
    assert_eq!(active(&warm, only(&warm, "harness")?)?, vec![kani()]);
    Ok(())
}

const A_GATED_ATTRIBUTE: &str = "\
pub fn ordinary() -> u32 { 1 }

#[cfg_attr(kani, test)]
pub fn attributed() {}

#[cfg(kani)]
pub fn harness() -> u32 { ordinary() }
";

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn an_attribute_macro_only_a_flipped_context_applies_does_not_stop_the_state_from_being_saved() -> Result<()> {
    let _serial = SERIAL.lock().unwrap();
    cordial::init_tracing();
    let root = fixture_with(A_GATED_ATTRIBUTE)?;
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = cordial::SavedState::at(state.path().join("analysis-state"));

    let (restored, cold) = run_saving(root.path(), &saved)?;
    assert!(!restored);
    assert_eq!(active(&cold, only(&cold, "harness")?)?, vec![kani()]);
    let (restored, warm) = run_saving(root.path(), &saved)?;
    assert!(restored, "the second run starts from the first's state");
    assert_eq!(active(&warm, only(&warm, "harness")?)?, vec![kani()]);
    Ok(())
}

/// What the state is saved with when a flipped context made each of these, in one fixture each. A save verifies
/// every persisted memo, so each is a shape that a memo of the flipped state can be in when the cfg is back.
const SHAPES: [(&str, &str); 8] = [
    (
        "an impl in a block inside a function only the context has",
        "pub fn ordinary() -> u32 { 1 }\n#[cfg(kani)]\npub fn harness() -> u32 {\n    struct Local(u32);\n    impl Local { #[kani::proof] fn get(&self) -> u32 { self.0 } }\n    Local(ordinary()).get()\n}\n",
    ),
    (
        "modules inside a module only the context has, with an impl whose item has an attribute path",
        "pub fn ordinary() -> u32 { 1 }\n#[cfg(kani)]\npub mod proofs {\n    pub mod inner {\n        pub struct Witness(pub u32);\n        impl Witness { #[kani::proof] pub fn get(&self) -> u32 { self.0 } }\n    }\n    pub fn harness() -> u32 { inner::Witness(1).get() }\n}\n",
    ),
    (
        "an attribute macro a macro_rules expansion applies only under the cfg",
        "macro_rules! make { () => { #[cfg_attr(kani, test)] pub fn made() {} } }\nmake!();\n#[cfg(kani)]\npub fn harness() {}\npub fn ordinary() {}\n",
    ),
    (
        "an enum, a trait and their members only the context has",
        "pub fn ordinary() {}\n#[cfg(kani)]\npub enum Choice { A, B(u32) }\n#[cfg(kani)]\npub trait Prove { fn go(&self) -> u32; fn also(&self) -> u32 { 1 } }\n#[cfg(kani)]\nimpl Prove for Choice { fn go(&self) -> u32 { 2 } }\n#[cfg(kani)]\npub fn harness() -> u32 { Choice::A.go() }\n",
    ),
    (
        "a block-local struct in a function that is there without the context, whose facts the context does not change",
        "pub fn alive() -> u32 {\n    #[cfg(kani)]\n    {\n        struct OnlyHere<T>(T);\n        let _ = OnlyHere(1u8);\n    }\n    1\n}\n#[cfg(kani)]\npub fn harness() -> u32 { alive() }\n",
    ),
    (
        "a function that is there without the context and uses a generic struct of a module only the context has",
        "pub fn alive() -> u32 {\n    #[cfg(kani)]\n    {\n        let _ = crate::proofs::W(1u8);\n    }\n    1\n}\n#[cfg(kani)]\npub mod proofs { pub struct W<T>(pub T); }\n#[cfg(kani)]\npub fn harness() -> u32 { alive() }\n",
    ),
    (
        "a block with an item and a macro call, in a function of a module only the context has",
        "pub fn alive() -> u32 { 1 }\n#[cfg(kani)]\npub mod proofs { pub fn inside() { { struct B; unknown_macro!(); } } }\n",
    ),
    (
        "a function whose body has closures, blocks and a macro call, only the context has",
        "pub fn ordinary() -> u32 { 1 }\n#[cfg(kani)]\npub fn harness() -> u32 {\n    let add = |x: u32| { let y = { x + 1 }; y };\n    let v = vec![1u32, 2, 3];\n    add(v.iter().sum::<u32>()) + ordinary()\n}\n",
    ),
];

/// Counts the item trees that execute: one a file, again for a file that tests an atom that was flipped, and
/// says how many there were when each context was done.
#[derive(Clone, Default)]
struct ItemTrees(Arc<AtomicUsize>, Arc<Mutex<Vec<usize>>>, Arc<Mutex<Vec<usize>>>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for ItemTrees {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        struct Message(String);
        impl tracing::field::Visit for Message {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                if field.name() == "message" {
                    self.0 = format!("{value:?}");
                }
            }
        }
        let mut message = Message(String::new());
        event.record(&mut message);
        if message.0.contains("cfg context analysed") {
            self.1.lock().unwrap().push(self.0.load(Ordering::SeqCst));
        }
        if message.0.contains("cfg is written into the ordinary crates") {
            self.2.lock().unwrap().push(self.0.load(Ordering::SeqCst));
        }
    }

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

fn two_gated_files() -> Result<tempfile::TempDir> {
    let root = fixture_with("mod a;\nmod b;\npub fn ordinary() {}\n")?;
    std::fs::write(
        root.path().join("src/a.rs"),
        "#[cfg(feature = \"a\")]\npub fn only_a() {}\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        root.path().join("src/b.rs"),
        "#[cfg(feature = \"b\")]\npub fn only_b() {}\n",
    )
    .into_diagnostic()?;
    Ok(root)
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_context_that_keeps_the_atoms_of_the_one_before_does_not_lower_their_files_again() -> Result<()> {
    let _serial = SERIAL.lock().unwrap();
    cordial::init_tracing();
    let root = two_gated_files()?;
    let a = SemanticContextKey::enabling(["feature=\"a\""]);
    let ab = SemanticContextKey::enabling(["feature=\"a\"", "feature=\"b\""]);
    let options = SemanticCargoOptions::default()
        .with_cfg_contexts(vec![a, ab])
        .with_cfg_planning(CfgPlanning::Explicit)
        .with_cfg_context_mode(CfgContextMode::Flips);
    let trees = ItemTrees::default();
    let subscriber = tracing_subscriber::registry().with(trees.clone());
    tracing::subscriber::with_default(subscriber, || -> Result<()> {
        let mut session = SourceDatabase::load_with(root.path(), &options).into_diagnostic()?;
        session
            .project(&CrateTarget::new("gated", root.path()), &[root.path().join("src/lib.rs")])
            .into_diagnostic()?;
        Ok(())
    })?;
    // The second context turns on `b` and leaves `a` as the first had it: only `b.rs` is lowered for it.
    let done = trees.1.lock().unwrap().clone();
    assert_eq!(done.len(), 2, "two contexts: {done:?}");
    assert_eq!(done[1] - done[0], 1, "`b.rs` and not `a.rs` again: {done:?}");
    Ok(())
}

#[test]
fn contexts_say_how_many_atoms_apart_they_are() {
    let ordinary = SemanticContextKey::ordinary();
    let a = SemanticContextKey::enabling(["feature=\"a\""]);
    let ab = SemanticContextKey::enabling(["feature=\"a\"", "feature=\"b\""]);
    let b = SemanticContextKey::enabling(["feature=\"b\""]);
    assert_eq!(ordinary.atoms_apart(&a), 1);
    assert_eq!(a.atoms_apart(&ab), 1, "one more atom on");
    assert_eq!(ordinary.atoms_apart(&ab), 2);
    assert_eq!(a.atoms_apart(&b), 2, "one off and one on");
    assert_eq!(ab.atoms_apart(&ab), 0);
}

/// Counts the bodies that execute, and how many did while the flipped crates were made again under the ordinary cfg.
#[derive(Clone, Default)]
struct Bodies {
    executed: Arc<AtomicUsize>,
    /// How many had executed when each settle began, and when it ended.
    settles: Arc<Mutex<Vec<(usize, usize)>>>,
    begun: Arc<Mutex<Option<usize>>>,
}

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Bodies {
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        _: &tracing::Id,
        _: tracing_subscriber::layer::Context<'_, S>,
    ) {
        if attrs.metadata().name() == "body_with_source_map_query" {
            self.executed.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        struct Message(String);
        impl tracing::field::Visit for Message {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                if field.name() == "message" {
                    self.0 = format!("{value:?}");
                }
            }
        }
        let mut message = Message(String::new());
        event.record(&mut message);
        let now = self.executed.load(Ordering::SeqCst);
        if message.0.contains("are about to be made again under the ordinary cfg") {
            *self.begun.lock().unwrap() = Some(now);
        } else if message.0.contains("are made again under the ordinary cfg")
            && let Some(begun) = self.begun.lock().unwrap().take()
        {
            self.settles.lock().unwrap().push((begun, now));
        }
    }
}

/// A body whose block exists only under the context: the body is made again with the ordinary cfg back, and the
/// block it made is deleted when it is.
const A_BLOCK_ONLY_THE_CONTEXT_HAS: &str = "\
pub fn ordinary() -> u32 { 1 }

pub fn with_a_block() -> u32 {
    #[cfg(kani)]
    {
        struct Local(u32);
        impl Local { fn get(&self) -> u32 { self.0 } }
        return Local(ordinary()).get();
    }
    #[allow(unreachable_code)]
    ordinary()
}
";

/// The same, with nothing in any body that the context changes.
const NO_BODY_THE_CONTEXT_CHANGES: &str = "\
pub fn ordinary() -> u32 { 1 }

pub fn plain() -> u32 {
    let block = { ordinary() };
    block
}

#[cfg(kani)]
pub fn harness() {}
";

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn the_bodies_a_flip_changed_are_made_again_before_anything_runs_in_parallel() -> Result<()> {
    let _serial = SERIAL.lock().unwrap();
    cordial::init_tracing();
    enum Condition {
        SomethingToCatch,
        NothingToSee,
    }
    for condition in [Condition::SomethingToCatch, Condition::NothingToSee] {
        let root = fixture_with(match condition {
            Condition::SomethingToCatch => A_BLOCK_ONLY_THE_CONTEXT_HAS,
            Condition::NothingToSee => NO_BODY_THE_CONTEXT_CHANGES,
        })?;
        let bodies = Bodies::default();
        let subscriber = tracing_subscriber::registry().with(bodies.clone());
        tracing::subscriber::with_default(subscriber, || -> Result<()> {
            let options = SemanticCargoOptions::default()
                .with_cfg_contexts(vec![kani()])
                .with_cfg_planning(CfgPlanning::Explicit)
                .with_cfg_context_mode(CfgContextMode::Flips);
            let mut session = SourceDatabase::load_with(root.path(), &options).into_diagnostic()?;
            session
                .project(&CrateTarget::new("gated", root.path()), &[root.path().join("src/lib.rs")])
                .into_diagnostic()?;
            Ok(())
        })?;
        let settles = bodies.settles.lock().unwrap().clone();
        let ended = settles.last().expect("the flipped crates were settled").1;
        // Nothing runs a body after the last settle: what runs then runs in parallel. With the block that only the
        // context has, a body made again there deletes a block that another thread read, which salsa does not allow
        // (this fixture panicked in a worker before the bodies were settled); with nothing of the kind there is
        // nothing to delete, and the same holds.
        assert_eq!(
            bodies.executed.load(Ordering::SeqCst),
            ended,
            "no body executes after the last settle ({settles:?})"
        );
    }
    Ok(())
}

/// Records the reads of tracked structs that are gone: a read of one whose slot was reused does not fail, it
/// returns another struct's fields, so a save that does not panic is not a save that read nothing it should not.
#[derive(Clone, Default)]
struct DeadReads(Arc<Mutex<Vec<String>>>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for DeadReads {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        struct Fields(String);
        impl tracing::field::Visit for Fields {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                if matches!(field.name(), "struct_kind" | "state") {
                    self.0.push_str(&format!("{}={value:?} ", field.name()));
                }
            }
        }
        if event.metadata().target() == "salsa::dead_read" {
            let mut fields = Fields(String::new());
            event.record(&mut fields);
            self.0.lock().unwrap().push(fields.0);
        }
    }
}

/// Every shape in one crate, each in a module of its own, so that one run covers them all.
fn all_the_shapes_in_one_crate() -> String {
    SHAPES
        .iter()
        .enumerate()
        .map(|(index, (_, source))| {
            let source = source.replace("crate::proofs::", &format!("crate::shape{index}::proofs::"));
            format!("pub mod shape{index} {{\n{source}\n}}\n")
        })
        .collect()
}

/// A run that flips a context and saves reads nothing of what only the flipped context made after it is gone, and does
/// not fail. The save verifies each memo of the flipped state, and a memo that read what is gone is what used to fail it;
/// each shape is one that such a memo can be in. One run over all of them: the cost of a run is the load of the sysroot
/// and not the size of the crate, and the verifying is in this run.
#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn no_shape_of_what_only_a_flipped_context_has_is_read_after_it_is_gone() -> Result<()> {
    let _serial = SERIAL.lock().unwrap();
    cordial::init_tracing();
    let root = fixture_with(&all_the_shapes_in_one_crate())?;
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = cordial::SavedState::at(state.path().join("analysis-state"));
    let reads = DeadReads::default();
    let subscriber = tracing_subscriber::registry().with(reads.clone());
    let mut failures = Vec::new();
    tracing::subscriber::with_default(subscriber, || {
        if let Err(error) = run_saving(root.path(), &saved) {
            failures.push(format!("the run failed: {error:?}"));
        }
    });
    failures.extend(reads.0.lock().unwrap().iter().map(|read| format!("read after it is gone: {read}")));
    assert!(failures.is_empty(), "{failures:#?}");
    Ok(())
}

/// Two contexts that each add a module and take it away again, so that the slot of the first one's module is the
/// second one's, with a function that is there without either and touches both and whose facts neither changes.
const TWO_CONTEXTS_EACH_WITH_A_MODULE: &str = "\
pub fn alive() -> u32 {
    #[cfg(kani)]
    {
        let _ = crate::proofs_k::K(1u8);
    }
    #[cfg(creusot)]
    {
        let _ = crate::proofs_c::C(1u8);
    }
    1
}
#[cfg(kani)]
pub mod proofs_k { pub struct K<T>(pub T); pub fn helper() {} }
#[cfg(creusot)]
pub mod proofs_c { pub struct C<T>(pub T); pub fn helper() {} }
";

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn what_two_contexts_add_in_turn_is_not_read_after_it_is_gone() -> Result<()> {
    let _serial = SERIAL.lock().unwrap();
    cordial::init_tracing();
    let root = fixture_with(TWO_CONTEXTS_EACH_WITH_A_MODULE)?;
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = cordial::SavedState::at(state.path().join("analysis-state"));
    let reads = DeadReads::default();
    let subscriber = tracing_subscriber::registry().with(reads.clone());
    let options = SemanticCargoOptions::default()
        .with_cfg_contexts(vec![kani(), creusot()])
        .with_cfg_planning(CfgPlanning::Explicit)
        .with_cfg_context_mode(CfgContextMode::Flips);
    tracing::subscriber::with_default(subscriber, || -> Result<()> {
        for _ in 0..2 {
            let mut session =
                SourceDatabase::load_saved(root.path(), &options, Some(saved.clone())).into_diagnostic()?;
            session
                .project(&CrateTarget::new("gated", root.path()), &[root.path().join("src/lib.rs")])
                .into_diagnostic()?;
        }
        Ok(())
    })?;
    let found = reads.0.lock().unwrap().clone();
    assert!(found.is_empty(), "{found:#?}");
    Ok(())
}

/// The `cordial::slots` events of a run: `when`, module ids, block ids, module cells, diagnostics cells.
#[derive(Clone, Default)]
struct Slots(Arc<Mutex<Vec<Reported>>>);

/// When it was said, and the module ids, block ids, module cells and diagnostics cells then.
type Reported = (String, [u64; 4]);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Slots {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        #[derive(Default)]
        struct Fields(String, [u64; 4]);
        impl tracing::field::Visit for Fields {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                if field.name() == "when" {
                    self.0 = format!("{value:?}").trim_matches('"').to_owned();
                }
            }
            fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
                if field.name() == "when" {
                    self.0 = value.to_owned();
                }
            }
            fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
                let at = ["module_ids", "block_ids", "module_cells", "diagnostics_cells"]
                    .iter()
                    .position(|name| *name == field.name());
                if let Some(at) = at {
                    self.1[at] = value;
                }
            }
        }
        if event.metadata().target() == "cordial::slots" {
            let mut fields = Fields::default();
            event.record(&mut fields);
            self.0.lock().unwrap().push((fields.0, fields.1));
        }
    }
}

/// The slots reported by a run of the one context over `library`, in the order they were reported.
fn slots_of_a_run(library: &str) -> Result<Vec<Reported>> {
    let root = fixture_with(library)?;
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = cordial::SavedState::at(state.path().join("analysis-state"));
    let slots = Slots::default();
    let subscriber = tracing_subscriber::registry().with(slots.clone());
    tracing::subscriber::with_default(subscriber, || run_saving(root.path(), &saved))?;
    let found = slots.0.lock().unwrap().clone();
    Ok(found)
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn the_ids_and_cells_a_run_holds_are_reported_and_a_module_only_a_context_has_adds_an_id() -> Result<()> {
    let _serial = SERIAL.lock().unwrap();
    cordial::init_tracing();
    let without = slots_of_a_run("pub fn alive() {}\n#[cfg(kani)]\npub fn harness() {}\n")?;
    let with = slots_of_a_run(
        "pub fn alive() {}\n#[cfg(kani)]\npub mod proofs { pub fn harness() {} }\n",
    )?;
    let finished = |slots: &[Reported]| {
        slots.iter().rev().find(|(when, _)| when == "flips finished").map(|(_, counts)| *counts)
    };
    let (without, with) = (finished(&without), finished(&with));
    let (Some(without), Some(with)) = (without, with) else {
        panic!("a run says what it holds when the flips are finished: {without:?} {with:?}");
    };
    assert!(without[0] > 0 && without[2] > 0, "module ids and cells are counted: {without:?}");
    assert_eq!(with[0], without[0] + 1, "the module only the context has is one more id that is kept");
    assert_eq!(with[1], without[1], "no block was added");
    Ok(())
}

/// The `cordial::eviction` events about what the map no longer declares: blocks held, blocks not declared.
#[derive(Clone, Default)]
struct Blocks(Arc<Mutex<Vec<(u64, u64)>>>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Blocks {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        #[derive(Default)]
        struct Fields([Option<u64>; 2]);
        impl tracing::field::Visit for Fields {
            fn record_debug(&mut self, _: &tracing::field::Field, _: &dyn std::fmt::Debug) {}
            fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
                match field.name() {
                    "blocks" => self.0[0] = Some(value),
                    "blocks_undeclared" => self.0[1] = Some(value),
                    _ => {}
                }
            }
        }
        let mut fields = Fields::default();
        event.record(&mut fields);
        if let [Some(held), Some(undeclared)] = fields.0 {
            self.0.lock().unwrap().push((held, undeclared));
        }
    }
}

/// The blocks held and not declared after a run of the one context over `library`.
fn blocks_of_a_run(library: &str) -> Result<(u64, u64)> {
    let root = fixture_with(library)?;
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = cordial::SavedState::at(state.path().join("analysis-state"));
    let blocks = Blocks::default();
    let subscriber = tracing_subscriber::registry().with(blocks.clone());
    tracing::subscriber::with_default(subscriber, || run_saving(root.path(), &saved))?;
    let said = blocks.0.lock().unwrap().clone();
    assert_eq!(said.len(), 1, "one report of what is held: {said:?}");
    Ok(said[0])
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_block_in_a_module_only_the_context_has_is_not_held_when_the_flips_are_finished() -> Result<()> {
    let _serial = SERIAL.lock().unwrap();
    cordial::init_tracing();
    let always = "pub fn alive() -> u32 { { struct S; let _ = S; } 1 }\n";
    let gated = "#[cfg(kani)]\npub mod proofs { pub fn f() { { struct B; let _ = B; } } }\n";
    let none = blocks_of_a_run("pub fn alive() -> u32 { 1 }\n")?;
    let (with_block, undeclared) = blocks_of_a_run(always)?;
    let (with_gated, undeclared_gated) = blocks_of_a_run(&format!("{always}{gated}"))?;
    assert_eq!(with_block, none.0 + 1, "a block that is always there is held");
    assert_eq!(
        (with_gated, undeclared, undeclared_gated),
        (with_block, 0, 0),
        "the block of the module only the context has is not left behind, so there is none to say is not declared"
    );
    Ok(())
}

/// What a context's harvest said about the functions it kept.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct KeptTotals {
    kept: u64,
    new_here: u64,
    /// The parts of the facts that differ, each with how many functions differ in it.
    differing: String,
    /// The sets of parts that differ together.
    together: String,
    /// How the definition was made, before and after, for each function whose identity changed.
    identity: String,
}

/// The totals of a harvest and the functions it named, with the part that differs and how.
#[derive(Clone, Default)]
struct KeptReport(Arc<Mutex<Vec<KeptTotals>>>, Arc<Mutex<Vec<(String, String)>>>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for KeptReport {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        #[derive(Default)]
        struct Fields {
            totals: KeptTotals,
            seen: [bool; 5],
        }
        impl tracing::field::Visit for Fields {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                let text = format!("{value:?}");
                match field.name() {
                    "differing" => (self.totals.differing, self.seen[2]) = (text, true),
                    "differing_together" => (self.totals.together, self.seen[3]) = (text, true),
                    "identity_changes" => (self.totals.identity, self.seen[4]) = (text, true),
                    _ => {}
                }
            }
            fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
                match field.name() {
                    "kept" => (self.totals.kept, self.seen[0]) = (value, true),
                    "new_here" => (self.totals.new_here, self.seen[1]) = (value, true),
                    _ => {}
                }
            }
        }
        if event.metadata().target() == "cordial::semantic::kept" {
            #[derive(Default)]
            struct Named(String, String);
            impl tracing::field::Visit for Named {
                fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                    match field.name() {
                        "parts" => self.0 = format!("{value:?}"),
                        "detail" => self.1 = format!("{value:?}"),
                        _ => {}
                    }
                }
            }
            let mut named = Named::default();
            event.record(&mut named);
            self.1.lock().unwrap().push((named.0, named.1));
            return;
        }
        if event.metadata().target() != "cordial::semantic::contexts" {
            return;
        }
        let mut fields = Fields::default();
        event.record(&mut fields);
        if fields.seen.iter().all(|seen| *seen) {
            self.0.lock().unwrap().push(fields.totals);
        }
    }
}

/// What a run reported of a context's kept functions, and the functions it named with the part that differs.
type KeptOfARun = (KeptTotals, Vec<(String, String)>);

/// What the one context `kani` kept of `library`: kept, new here, and the parts that differ.
fn kept_of_a_run(library: &str) -> Result<KeptOfARun> {
    let root = fixture_with(library)?;
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = cordial::SavedState::at(state.path().join("analysis-state"));
    let report = KeptReport::default();
    let subscriber = tracing_subscriber::registry().with(report.clone());
    tracing::subscriber::with_default(subscriber, || run_saving(root.path(), &saved))?;
    let said = report.0.lock().unwrap().clone();
    assert_eq!(said.len(), 1, "one harvest was reported: {said:?}");
    let named = report.1.lock().unwrap().clone();
    Ok((said[0].clone(), named))
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_function_the_context_does_not_change_is_not_kept_and_one_whose_call_resolves_elsewhere_is_said_to_differ_in_its_calls()
-> Result<()> {
    let _serial = SERIAL.lock().unwrap();
    cordial::init_tracing();
    // Nothing the context flips is read by `plain`: it is not kept, and nothing differs.
    let unaffected =
        kept_of_a_run("pub fn plain() -> u32 { 1 }\n#[cfg(kani)]\npub fn harness() -> u32 { 2 }\n")?;
    assert_eq!(
        (unaffected.0.kept, unaffected.0.new_here, unaffected.0.differing.as_str()),
        (1, 1, "{}"),
        "only `harness` is kept, and it is new here"
    );
    // `caller` calls the `helper` that the context's cfg selects, which is another function: it is kept, and what
    // differs is the target of its call and nothing else.
    let changed = kept_of_a_run(
        "#[cfg(not(kani))]\npub fn helper() -> u32 { 1 }\n#[cfg(kani)]\npub fn helper() -> u32 { 2 }\npub fn caller() -> u32 { helper() }\n",
    )?;
    assert_eq!(
        (changed.0.kept, changed.0.new_here, changed.0.differing.as_str(), changed.0.together.as_str()),
        (2, 1, "{CallsTarget: 1}", "{\"[CallsTarget]\": 1}"),
        "`helper` is new here, `caller` is kept, and its call is what differs"
    );
    assert_eq!(changed.0.identity, "{}", "no identity changed");
    assert_eq!(changed.1.len(), 1, "the one function the ordinary build has is named: {:?}", changed.1);
    assert_eq!(changed.1[0].0, "[CallsTarget]");
    assert!(changed.1[0].1.contains("CallsTarget: ") && changed.1[0].1.contains("Function("), "and what its call was and is: {:?}", changed.1);
    assert!(unaffected.1.is_empty(), "nothing is named when nothing differs: {:?}", unaffected.1);
    Ok(())
}

/// The totals a harvest reported (functions, files taken as they are), and the functions each file has.
type FunctionsByFile = ((u64, u64), Vec<(String, u64)>);

/// What the context `-unix` says it has of a crate whose root is `library` and which has `src/platform.rs`: the
/// functions it has, and the files it took as they are.
fn functions_of_a_crate_without_unix(mode: CfgContextMode, library: &str) -> Result<FunctionsByFile> {
    let root = fixture_with(library)?;
    std::fs::write(
        root.path().join("src/platform.rs"),
        "pub fn only_unix() -> u32 { 1 }\npub fn also_unix() -> u32 { 2 }\n",
    )
    .into_diagnostic()?;
    let options = SemanticCargoOptions::default()
        .with_cfg_contexts(vec![SemanticContextKey::new(Vec::<String>::new(), ["unix"])])
        .with_cfg_planning(CfgPlanning::Explicit)
        .with_cfg_context_mode(mode);
    let report = FunctionsReport::default();
    let subscriber = tracing_subscriber::registry().with(report.clone());
    tracing::subscriber::with_default(subscriber, || -> Result<()> {
        let mut session = SourceDatabase::load_saved(root.path(), &options, None).into_diagnostic()?;
        session
            .project(
                &CrateTarget::new("gated", root.path()),
                &[root.path().join("src/lib.rs"), root.path().join("src/platform.rs")],
            )
            .into_diagnostic()?;
        Ok(())
    })?;
    let said = report.0.lock().unwrap().clone();
    assert_eq!(said.len(), 1, "one harvest was reported: {said:?}");
    let mut files = report.1.lock().unwrap().clone();
    files.sort();
    Ok(((said[0].0, said[0].1), files))
}

/// `platform` is declared, and only where `unix` is set.
fn functions_without_unix(mode: CfgContextMode) -> Result<FunctionsByFile> {
    functions_of_a_crate_without_unix(mode, "#[cfg(unix)]\nmod platform;\npub fn alive() -> u32 { 1 }\n")
}

/// `platform.rs` is a file no module declares, as `src/quality/exceptions.rs` of `elicit_doc` is.
fn functions_of_a_file_no_module_declares(mode: CfgContextMode) -> Result<FunctionsByFile> {
    functions_of_a_crate_without_unix(mode, "pub fn alive() -> u32 { 1 }\n")
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_file_no_module_declares_has_no_active_functions_in_a_context() -> Result<()> {
    let _serial = SERIAL.lock().unwrap();
    cordial::init_tracing();
    let (totals, files) = functions_of_a_file_no_module_declares(CfgContextMode::Flips)?;
    assert_eq!(totals, (1, 0));
    assert_eq!(files, [("lib.rs".to_owned(), 1), ("platform.rs".to_owned(), 0)]);
    Ok(())
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn the_copy_design_counts_the_functions_of_a_file_no_module_declares_as_active() -> Result<()> {
    let _serial = SERIAL.lock().unwrap();
    cordial::init_tracing();
    // The 13 functions of `src/quality/exceptions.rs` that `elicit_doc` had active in the copies run and not in the
    // flips run: nothing declares the file, and the copy takes it as it is. Removed with the copy design.
    let (totals, files) = functions_of_a_file_no_module_declares(CfgContextMode::Copies)?;
    assert_eq!(totals, (3, 2), "no gate names `unix`, so both files are taken as they are");
    assert_eq!(files, [("lib.rs".to_owned(), 1), ("platform.rs".to_owned(), 2)]);
    Ok(())
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_file_only_a_disabled_atom_declares_has_no_active_functions_in_the_context_that_disables_it() -> Result<()> {
    let _serial = SERIAL.lock().unwrap();
    cordial::init_tracing();
    // The module is not in the crate without `unix`, so its two functions are not active there: only `alive` is.
    let (totals, files) = functions_without_unix(CfgContextMode::Flips)?;
    assert_eq!(totals, (1, 0), "functions, and files taken as they are");
    assert_eq!(
        files,
        [("lib.rs".to_owned(), 1), ("platform.rs".to_owned(), 0)],
        "the log says which file has the function"
    );
    Ok(())
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn the_copy_design_counts_the_functions_of_a_file_a_context_disables_as_active() -> Result<()> {
    let _serial = SERIAL.lock().unwrap();
    cordial::init_tracing();
    // Why the third amenable context had 216 active functions in the copies run and 39 in the flips run: the copy
    // takes a file whose words no gate names as it is, and counts every function in it active, whether the crate has
    // the module or not. Removed with the copy design (Phase 3), with the parity it was the exception to.
    let (totals, files) = functions_without_unix(CfgContextMode::Copies)?;
    assert_eq!(totals, (3, 1), "functions, and files taken as they are");
    assert_eq!(
        files,
        [("lib.rs".to_owned(), 1), ("platform.rs".to_owned(), 2)],
        "and the file that the crate does not have is the one with the two"
    );
    Ok(())
}

/// The harvest's totals (functions, aliased files, kept) and, per file, the functions it has in the context.
#[derive(Clone, Default)]
struct FunctionsReport(Arc<Mutex<Vec<(u64, u64, u64)>>>, Arc<Mutex<Vec<(String, u64)>>>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for FunctionsReport {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        #[derive(Default)]
        struct Fields {
            totals: [Option<u64>; 3],
            file: Option<String>,
            file_functions: Option<u64>,
        }
        impl tracing::field::Visit for Fields {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                if field.name() == "file" {
                    self.file = Some(format!("{value:?}"));
                }
            }
            fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
                match field.name() {
                    "functions" => {
                        self.totals[0] = Some(value);
                        self.file_functions = Some(value);
                    }
                    "aliased_files" => self.totals[1] = Some(value),
                    "kept" => self.totals[2] = Some(value),
                    _ => {}
                }
            }
        }
        let mut fields = Fields::default();
        match event.metadata().target() {
            "cordial::semantic::contexts" => {
                event.record(&mut fields);
                if let [Some(a), Some(b), Some(c)] = fields.totals {
                    self.0.lock().unwrap().push((a, b, c));
                }
            }
            "cordial::semantic::files" => {
                event.record(&mut fields);
                if let (Some(file), Some(functions)) = (fields.file, fields.file_functions) {
                    let name = file.trim_matches('"').rsplit('/').next().unwrap_or_default().to_owned();
                    self.1.lock().unwrap().push((name, functions));
                }
            }
            _ => {}
        }
    }
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_cfg_attr_in_front_of_an_attribute_macro_changes_the_identity_of_the_function_and_nothing_else() -> Result<()> {
    let _serial = SERIAL.lock().unwrap();
    cordial::init_tracing();
    // The macro call is made from the item and the index of the attribute among its attributes, so the context that
    // adds one in front of it makes another call, and the function it expands to is made in another file: the same
    // function, written in one place, has another identity, and what it says is the same.
    let (totals, named) = kept_of_a_run(
        "#[rustc_builtin_macro]\npub macro global_allocator($item:item) {}\n#[cfg_attr(kani, inline)]\n#[global_allocator]\npub fn f() -> u32 { 1 }\n",
    )?;
    assert_eq!((totals.kept, totals.new_here), (1, 0), "`f` is kept, and the ordinary build has it");
    assert_eq!(totals.differing, "{Definition: 1}");
    assert_eq!(totals.together, "{\"[Definition]\": 1}");
    assert_eq!(named.len(), 1, "{named:?}");
    assert!(totals.identity.contains("Generated"), "the definition is one a macro made: {}", totals.identity);
    Ok(())
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn an_attribute_macro_the_context_turns_off_makes_the_function_one_that_is_written_and_not_one_a_macro_made() -> Result<()> {
    let _serial = SERIAL.lock().unwrap();
    cordial::init_tracing();
    // `#[cfg_attr(not(kani), tracing::instrument)]` is on thousands of functions of amenable: without `kani` a macro
    // wraps the function, and with it the function stands as it is written. The context's facts are those of the
    // function as written, which are not the ordinary ones, and keeping them is right.
    let (totals, _) = kept_of_a_run(
        "#[rustc_builtin_macro]\npub macro global_allocator($item:item) {}\n#[cfg_attr(not(kani), global_allocator)]\npub fn f() -> u32 { 1 }\n",
    )?;
    assert_eq!((totals.kept, totals.new_here), (1, 0), "`f` is kept, and the ordinary build has it");
    assert_eq!(totals.identity, "{\"Generated -> Written\": 1}", "a macro made it, and now it is written");
    Ok(())
}

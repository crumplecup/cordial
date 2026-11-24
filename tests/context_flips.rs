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
const SHAPES: [(&str, &str); 5] = [
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
        "a function whose body has closures, blocks and a macro call, only the context has",
        "pub fn ordinary() -> u32 { 1 }\n#[cfg(kani)]\npub fn harness() -> u32 {\n    let add = |x: u32| { let y = { x + 1 }; y };\n    let v = vec![1u32, 2, 3];\n    add(v.iter().sum::<u32>()) + ordinary()\n}\n",
    ),
];

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn each_shape_of_what_only_a_flipped_context_has_leaves_a_state_that_saves() -> Result<()> {
    let _serial = SERIAL.lock().unwrap();
    cordial::init_tracing();
    let mut failures = Vec::new();
    for (shape, source) in SHAPES {
        let root = fixture_with(source)?;
        let state = tempfile::tempdir().into_diagnostic()?;
        let saved = cordial::SavedState::at(state.path().join("analysis-state"));
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_saving(root.path(), &saved).and_then(|(_, _)| run_saving(root.path(), &saved))
        }));
        match outcome {
            Ok(Ok((restored, _))) if restored => {}
            Ok(Ok(_)) => failures.push(format!("{shape}: the second run did not restore")),
            Ok(Err(error)) => failures.push(format!("{shape}: {error:?}")),
            Err(_) => failures.push(format!("{shape}: a panic")),
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
    Ok(())
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn an_impl_in_a_block_inside_a_function_only_a_flipped_context_has_leaves_a_state_that_saves() -> Result<()> {
    let _serial = SERIAL.lock().unwrap();
    cordial::init_tracing();
    let root = fixture_with(SHAPES[0].1)?;
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = cordial::SavedState::at(state.path().join("analysis-state"));
    let (restored, _) = run_saving(root.path(), &saved)?;
    assert!(!restored);
    let (restored, _) = run_saving(root.path(), &saved)?;
    assert!(restored);
    Ok(())
}

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

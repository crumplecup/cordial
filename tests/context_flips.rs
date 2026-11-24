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

//! A cfg context's crates and file copies are saved with the database and taken up again by
//! the next run, so what was computed for them is not computed again.

mod support {
    pub mod semantic_metrics;
}

use cordial::{
    CfgPlanning, CrateIr, CrateTarget, IrView, SavedState, SemanticCargoOptions,
    SemanticContextKey, SourceDatabase,
};
use miette::{IntoDiagnostic, Result};
use std::path::Path;
use tracing_subscriber::prelude::*;

const LIBRARY: &str = "\
pub fn ordinary() {}

#[cfg(kani)]
pub fn harness() { ordinary(); }

#[cfg(not(kani))]
pub fn unless_kani() {}
";

fn fixture() -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir_all(root.path().join("src")).into_diagnostic()?;
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='gated'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    std::fs::write(root.path().join("src/lib.rs"), LIBRARY).into_diagnostic()?;
    Ok(root)
}

fn run(root: &Path, saved: &SavedState) -> Result<(bool, CrateIr)> {
    let options = SemanticCargoOptions::default()
        .with_cfg_contexts(vec![SemanticContextKey::enabling(["kani"])])
        .with_cfg_planning(CfgPlanning::Explicit);
    let mut session =
        SourceDatabase::load_saved(root, &options, Some(saved.clone())).into_diagnostic()?;
    let restored = session.restored();
    let graph = session
        .project(&CrateTarget::new("gated", root), &[root.join("src/lib.rs")])
        .into_diagnostic()?;
    Ok((restored, graph))
}

fn functions(graph: &CrateIr) -> Vec<String> {
    let mut names: Vec<String> = graph
        .nodes_matching(&cordial::BasicQuery::items())
        .into_iter()
        .filter_map(|node| node.weight().name().clone())
        .collect();
    names.sort();
    names
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_restored_run_takes_the_contexts_up_again_and_computes_nothing_for_them() -> Result<()> {
    cordial::init_tracing();
    let metrics = support::semantic_metrics::QueryMetrics::default();
    let subscriber = tracing_subscriber::registry()
        .with(metrics.clone())
        .with(cordial::tracing_fmt_layer());
    let _subscriber = tracing::subscriber::set_default(subscriber);
    let root = fixture()?;
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = SavedState::at(state.path().join("analysis-state"));

    let (restored, cold) = run(root.path(), &saved)?;
    assert!(!restored);
    let cold_counts = metrics.snapshot();
    assert!(!cold_counts.is_empty());

    let (restored, warm) = run(root.path(), &saved)?;
    assert!(restored, "the second run starts from the first's state");
    assert_eq!(functions(&warm), functions(&cold), "the same answer");
    // What the restored run computed, by query.
    let mut recomputed: std::collections::BTreeMap<String, usize> = metrics
        .snapshot()
        .into_iter()
        .filter_map(|(key, count)| {
            let before = cold_counts.get(&key).copied().unwrap_or(0);
            (count > before).then(|| (key, count - before))
        })
        .collect();
    // The joins over a crate's files are not saved; they are rebuilt from the saved parts.
    const JOINS: [&str; 5] = [
        "antipattern_crate_facts:",
        "architecture_catalog:",
        "panic_graph:",
        "panic_reachable:",
        "signature_index:",
    ];
    recomputed.retain(|key, _| !JOINS.iter().any(|join| key.starts_with(join)));
    assert!(
        recomputed.is_empty(),
        "the restored run computed again: {recomputed:#?}"
    );
    Ok(())
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn an_edit_between_runs_reaches_the_taken_up_contexts() -> Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = SavedState::at(state.path().join("analysis-state"));
    let (_, cold) = run(root.path(), &saved)?;
    assert!(functions(&cold).contains(&"harness".to_owned()));

    // A function gated on kani is added; the context's copy of the file follows the edit.
    let library = root.path().join("src/lib.rs");
    std::fs::write(
        &library,
        format!("{LIBRARY}\n#[cfg(kani)]\npub fn second_harness() {{ ordinary(); }}\n"),
    )
    .into_diagnostic()?;
    let (restored, warm) = run(root.path(), &saved)?;
    assert!(restored);
    let names = functions(&warm);
    assert!(
        names.contains(&"second_harness".to_owned()),
        "the context sees the edit: {names:?}"
    );
    assert!(names.contains(&"harness".to_owned()));
    Ok(())
}

/// A workspace of two local crates, one depending on the other, each with code gated on cfg.
fn two_crates() -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    for directory in ["src", "inner/src"] {
        std::fs::create_dir_all(root.path().join(directory)).into_diagnostic()?;
    }
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='gated'\nversion='0.1.0'\nedition='2024'\n\
         [dependencies]\ninner = { path = \"inner\" }\n[workspace]\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        root.path().join("inner/Cargo.toml"),
        "[package]\nname='inner'\nversion='0.1.0'\nedition='2024'\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        root.path().join("inner/src/lib.rs"),
        "pub fn helper() {}\n#[cfg(kani)]\npub fn kani_only() { helper(); }\n\
         #[cfg(creusot)]\npub fn creusot_only() { helper(); }\n",
    )
    .into_diagnostic()?;
    std::fs::create_dir_all(root.path().join("tests")).into_diagnostic()?;
    std::fs::write(
        root.path().join("tests/integration.rs"),
        "#[test]\nfn uses_the_library() { gated::ordinary(); }\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        root.path().join("src/lib.rs"),
        "pub fn ordinary() { inner::helper(); }\n#[cfg(kani)]\npub fn harness() { inner::kani_only(); }\n\
         #[cfg(creusot)]\npub fn proof() { inner::creusot_only(); }\n",
    )
    .into_diagnostic()?;
    Ok(root)
}

fn run_many(root: &Path, saved: &SavedState) -> Result<bool> {
    let options = SemanticCargoOptions::default()
        .with_cfg_contexts(vec![
            SemanticContextKey::enabling(["kani"]),
            SemanticContextKey::enabling(["creusot"]),
            SemanticContextKey::enabling(["kani", "creusot"]),
        ])
        .with_cfg_planning(CfgPlanning::Explicit);
    let mut session =
        SourceDatabase::load_saved(root, &options, Some(saved.clone())).into_diagnostic()?;
    let restored = session.restored();
    session
        .project(&CrateTarget::new("gated", root), &[root.join("src/lib.rs")])
        .into_diagnostic()?;
    Ok(restored)
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_restored_run_reuses_what_several_contexts_over_dependent_crates_computed() -> Result<()> {
    cordial::init_tracing();
    let metrics = support::semantic_metrics::QueryMetrics::default();
    let subscriber = tracing_subscriber::registry()
        .with(metrics.clone())
        .with(cordial::tracing_fmt_layer());
    let _subscriber = tracing::subscriber::set_default(subscriber);
    let root = two_crates()?;
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = SavedState::at(state.path().join("analysis-state"));

    assert!(!run_many(root.path(), &saved)?);
    let cold_counts = metrics.snapshot();
    assert!(!cold_counts.is_empty());

    assert!(
        run_many(root.path(), &saved)?,
        "the second run starts from the first's state"
    );
    const JOINS: [&str; 5] = [
        "antipattern_crate_facts:",
        "architecture_catalog:",
        "panic_graph:",
        "panic_reachable:",
        "signature_index:",
    ];
    let recomputed: std::collections::BTreeMap<String, usize> = metrics
        .snapshot()
        .into_iter()
        .filter_map(|(key, count)| {
            let before = cold_counts.get(&key).copied().unwrap_or(0);
            (count > before && !JOINS.iter().any(|join| key.starts_with(join)))
                .then(|| (key, count - before))
        })
        .collect();
    assert!(
        recomputed.is_empty(),
        "the restored run computed again: {recomputed:#?}"
    );
    Ok(())
}

/// "N evaluated for M" from the run's evidence line, since the last call.
fn workspace_evidence() -> Option<(usize, usize)> {
    cordial::host::take_notes()
        .events()
        .iter()
        .find_map(|event| {
            let rest = event.strip_prefix("function evidence (workspace): ")?;
            let mut words = rest.split_whitespace();
            Some((words.next()?.parse().ok()?, words.nth(2)?.parse().ok()?))
        })
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_restored_run_evaluates_no_ordinary_function_though_contexts_copy_the_crates() -> Result<()> {
    cordial::init_tracing();
    let root = two_crates()?;
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = SavedState::at(state.path().join("analysis-state"));
    let _ = workspace_evidence();

    assert!(!run_many(root.path(), &saved)?);
    let (cold, functions) =
        workspace_evidence().ok_or_else(|| miette::miette!("the cold run logs its evidence"))?;
    assert_eq!(cold, functions);

    assert!(run_many(root.path(), &saved)?);
    let (again, total) = workspace_evidence()
        .ok_or_else(|| miette::miette!("the restored run logs its evidence"))?;
    assert_eq!(total, functions);
    assert_eq!(
        again, 0,
        "{again} of {total} ordinary functions were evaluated again in the restored run"
    );
    Ok(())
}

/// Three local crates in a chain, `gated` -> `inner` -> `leaf`, where `inner` uses what `leaf`
/// defines (a type with a derive, a function it calls), each with code gated on cfg.
fn chain_of_crates() -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    for directory in ["src", "inner/src", "leaf/src"] {
        std::fs::create_dir_all(root.path().join(directory)).into_diagnostic()?;
    }
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='gated'\nversion='0.1.0'\nedition='2024'\n\
         [dependencies]\ninner = { path = \"inner\" }\n[workspace]\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        root.path().join("inner/Cargo.toml"),
        "[package]\nname='inner'\nversion='0.1.0'\nedition='2024'\n\
         [dependencies]\nleaf = { path = \"../leaf\" }\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        root.path().join("leaf/Cargo.toml"),
        "[package]\nname='leaf'\nversion='0.1.0'\nedition='2024'\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        root.path().join("leaf/src/lib.rs"),
        "#[derive(Clone, Debug)]\npub struct Item(pub u8);\npub fn make() -> Item { Item(1) }\n\
         #[cfg(kani)]\npub fn kani_leaf() {}\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        root.path().join("inner/src/lib.rs"),
        "#[derive(Clone, Debug)]\npub struct Wrapper(pub leaf::Item);\n\
         pub fn wrap() -> Wrapper { Wrapper(leaf::make()) }\n\
         #[cfg(kani)]\npub fn kani_inner() { leaf::kani_leaf(); }\n\
         #[cfg(creusot)]\npub fn creusot_inner() {}\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        root.path().join("src/lib.rs"),
        "pub fn ordinary() -> inner::Wrapper { inner::wrap().clone() }\n\
         #[cfg(kani)]\npub fn harness() { inner::kani_inner(); }\n",
    )
    .into_diagnostic()?;
    Ok(root)
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_restored_run_evaluates_no_function_of_crates_that_use_each_others_types() -> Result<()> {
    cordial::init_tracing();
    let root = chain_of_crates()?;
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = SavedState::at(state.path().join("analysis-state"));
    let _ = workspace_evidence();

    assert!(!run_many(root.path(), &saved)?);
    let (cold, functions) =
        workspace_evidence().ok_or_else(|| miette::miette!("the cold run logs its evidence"))?;
    assert_eq!(cold, functions);

    assert!(run_many(root.path(), &saved)?);
    let (again, total) = workspace_evidence()
        .ok_or_else(|| miette::miette!("the restored run logs its evidence"))?;
    assert_eq!(total, functions);
    assert_eq!(
        again, 0,
        "{again} of {total} functions were evaluated again"
    );
    Ok(())
}

/// What the save said about the memos that reach a crate a context added: for each memo, its
/// key, and for each persisted evidence key, what function it is for.
#[derive(Default, Clone)]
struct SaveEvents {
    reaches: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    described: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, String>>>,
}

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for SaveEvents {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        #[derive(Default)]
        struct Fields(std::collections::HashMap<String, String>);
        impl tracing::field::Visit for Fields {
            fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
                self.0.insert(field.name().to_owned(), value.to_string());
            }
            fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
                self.0.insert(field.name().to_owned(), value.to_owned());
            }
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                self.0.insert(field.name().to_owned(), format!("{value:?}"));
            }
        }
        let target = event.metadata().target();
        if target != "salsa::explain" && target != "cordial::keys" {
            return;
        }
        let mut fields = Fields::default();
        event.record(&mut fields);
        let (Ok(mut reaches), Ok(mut described)) = (self.reaches.lock(), self.described.lock())
        else {
            return;
        };
        if target == "cordial::keys" {
            if let (Some(key), Some(description)) =
                (fields.0.get("key"), fields.0.get("description"))
            {
                described.insert(key.clone(), description.clone());
            }
        } else if fields.0.get("ingredient").map(String::as_str) == Some("persisted_evidence")
            && let Some(memo) = fields.0.get("memo")
        {
            // The memo's key is written `ingredient:key`.
            reaches.push(memo.rsplit(':').next().unwrap_or_default().to_owned());
        }
    }
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn an_ordinary_functions_saved_memo_depends_on_nothing_a_context_made() -> Result<()> {
    use tracing_subscriber::prelude::*;
    cordial::init_tracing();
    let events = SaveEvents::default();
    let subscriber = tracing_subscriber::registry().with(events.clone());
    let _subscriber = tracing::subscriber::set_default(subscriber);
    let root = chain_of_crates()?;
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = SavedState::at(state.path().join("analysis-state"));

    assert!(!run_many(root.path(), &saved)?);

    let reaches = events
        .reaches
        .lock()
        .map(|reaches| reaches.clone())
        .unwrap_or_default();
    let described = events
        .described
        .lock()
        .map(|d| d.clone())
        .unwrap_or_default();
    assert!(
        !described.is_empty(),
        "the run said what each persisted evidence key is for"
    );
    // Without this the test would pass whatever the truth: the memos of the contexts' own functions
    // do reach the crates their context added, and the save must have said so.
    assert!(
        !reaches.is_empty(),
        "the save names the memos that reach a crate a context added"
    );
    let ordinary: Vec<String> = reaches
        .iter()
        .filter_map(|key| described.get(key))
        .filter(|description| !description.contains("/__cordial_context_"))
        .cloned()
        .collect();
    assert!(
        ordinary.is_empty(),
        "memos of ordinary functions reach a crate a context added: {ordinary:?} (of {} memos \
         that reach one)",
        reaches.len()
    );
    Ok(())
}

/// How many times Salsa executed each query, by name, from its own events.
#[derive(Default, Clone)]
struct Executions(std::sync::Arc<std::sync::Mutex<std::collections::BTreeMap<String, usize>>>);

impl<S> tracing_subscriber::Layer<S> for Executions
where
    S: tracing::Subscriber + for<'lookup> tracing_subscriber::registry::LookupSpan<'lookup>,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        context: tracing_subscriber::layer::Context<'_, S>,
    ) {
        if event.metadata().target() != "salsa::function::execute" {
            return;
        }
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
        let query = message.0.split('(').next().unwrap_or_default();
        // The innermost span of cordial's own that the query runs under says which step asked.
        let step = context
            .event_scope(event)
            .and_then(|scope| {
                scope
                    .into_iter()
                    .find(|span| span.metadata().target().starts_with("cordial"))
                    .map(|span| span.name())
            })
            .unwrap_or("outside cordial");
        let name = format!("{step} > {query}");
        if let Ok(mut counts) = self.0.lock() {
            *counts.entry(name).or_default() += 1;
        }
    }
}

impl Executions {
    fn take(&self) -> std::collections::BTreeMap<String, usize> {
        self.0
            .lock()
            .map(|mut counts| std::mem::take(&mut *counts))
            .unwrap_or_default()
    }
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_restored_run_executes_few_rust_analyzer_queries() -> Result<()> {
    use tracing_subscriber::prelude::*;
    cordial::init_tracing();
    let executions = Executions::default();
    let subscriber = tracing_subscriber::registry().with(executions.clone());
    let _subscriber = tracing::subscriber::set_default(subscriber);
    let root = chain_of_crates()?;
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = SavedState::at(state.path().join("analysis-state"));

    assert!(!run_many(root.path(), &saved)?);
    let cold = executions.take();
    assert!(run_many(root.path(), &saved)?);
    let restored = executions.take();

    let mut by_step = std::collections::BTreeMap::<&str, usize>::new();
    for (name, count) in &restored {
        *by_step
            .entry(name.split(" > ").next().unwrap_or_default())
            .or_default() += count;
    }
    // What a restored run still asks rust-analyzer for is what it takes to name each function
    // (parse, item trees, attributes, macro expansion) so its saved evidence can be found: the
    // steps below. Anything else asking for analysis in a restored run is new work to account for.
    const NAMING: [&str; 15] = [
        "local_functions",
        "derived_site",
        "inner_",
        "declared_in",
        "resolve",
        "harvest",
        "written_in_file",
        "generated_in",
        // One each: the joins over a crate's files, rebuilt from the saved parts, the source
        // captured for the run, and the warm-up's own question of the saved evidence.
        "antipattern_crate_facts",
        "architecture_catalog",
        "captured_source",
        "panic_graph",
        "panic_reachable",
        "persisted_evidence",
        "signature_index",
    ];
    let others: Vec<_> = by_step
        .keys()
        .filter(|step| !NAMING.contains(step))
        .collect();
    assert!(
        others.is_empty(),
        "a restored run executed queries under steps that do not name functions: {others:?}"
    );
    assert!(
        restored.values().sum::<usize>() < cold.values().sum::<usize>(),
        "a restored run executes fewer queries than a cold one"
    );
    Ok(())
}

/// The descriptions of the functions a run asked evidence for, from its `cordial::keys` events.
fn described_functions(run_it: impl FnOnce() -> Result<()>) -> Result<Vec<String>> {
    let events = SaveEvents::default();
    let subscriber = tracing_subscriber::registry().with(events.clone());
    let guard = tracing::subscriber::set_default(subscriber);
    run_it()?;
    drop(guard);
    let mut described: Vec<String> = events
        .described
        .lock()
        .map(|described| described.values().cloned().collect())
        .unwrap_or_default();
    described.sort();
    Ok(described)
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_comment_above_a_function_does_not_change_what_describes_it() -> Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let library = root.path().join("src/lib.rs");
    let state = tempfile::tempdir().into_diagnostic()?;
    let before = described_functions(|| {
        run(root.path(), &SavedState::at(state.path().join("one"))).map(|_| ())
    })?;
    assert!(
        !before.is_empty(),
        "the run described the functions it evaluated"
    );

    let text = std::fs::read_to_string(&library).into_diagnostic()?;
    std::fs::write(&library, format!("// a comment above everything\n{text}")).into_diagnostic()?;
    let after = described_functions(|| {
        run(root.path(), &SavedState::at(state.path().join("two"))).map(|_| ())
    })?;
    assert_eq!(
        before, after,
        "a comment moved what describes the functions below it"
    );
    Ok(())
}

const LOOK_ALIKES: &str = "\
pub struct A;
pub struct B;
impl A { pub fn new() -> A { A } }
impl B { pub fn new() -> B { B } }
pub mod inner { pub fn new() {} }
pub trait Make { fn new() -> Self; }
impl Make for A { fn new() -> A { A } }
#[cfg(kani)]
pub fn twin() {}
#[cfg(not(kani))]
pub fn twin() {}
pub fn outer() { fn new() {} new(); }
";

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn functions_that_look_alike_are_told_apart_and_found_again() -> Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let library = root.path().join("src/lib.rs");
    std::fs::write(&library, LOOK_ALIKES).into_diagnostic()?;
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = SavedState::at(state.path().join("analysis-state"));

    let described = described_functions(|| run(root.path(), &saved).map(|_| ()))?;
    let (cold, total) =
        workspace_evidence().ok_or_else(|| miette::miette!("the cold run logs its evidence"))?;
    assert_eq!(cold, total);
    let real: Vec<_> = described
        .iter()
        .filter(|text| text.contains("|real|"))
        .collect();
    let distinct: std::collections::BTreeSet<_> = real.iter().collect();
    assert_eq!(
        real.len(),
        distinct.len(),
        "two functions share a description: {real:#?}"
    );
    assert!(real.len() >= 8, "every function was described: {real:#?}");

    let (restored, _) = run(root.path(), &saved)?;
    assert!(restored);
    let (again, total) = workspace_evidence()
        .ok_or_else(|| miette::miette!("the restored run logs its evidence"))?;
    assert_eq!(
        again, 0,
        "{again} of {total} look-alike functions were evaluated again"
    );
    Ok(())
}

/// Each function's evidence as JSON, by label, with the source ranges of its calls removed.
fn evidence_by_label(
    graph: &CrateIr,
    without_ranges: bool,
) -> Result<std::collections::BTreeMap<String, serde_json::Value>> {
    let query = cordial::QueryBuilder::new()
        .has_attr(cordial::ATTR_SEMANTIC_FUNCTION)
        .build();
    let mut found = std::collections::BTreeMap::new();
    for node in graph.nodes_matching(&query) {
        if let Some(facts) = graph.semantic_function(node.id()).into_diagnostic()? {
            let mut value = serde_json::to_value(&facts).into_diagnostic()?;
            if without_ranges
                && let Some(calls) = value
                    .get_mut("calls")
                    .and_then(|calls| calls.as_array_mut())
            {
                for call in calls {
                    if let Some(call) = call.as_object_mut() {
                        call.remove("range");
                    }
                }
            }
            found.insert(facts.label().clone(), value);
        }
    }
    Ok(found)
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_comment_above_a_function_changes_nothing_in_the_evidence_but_positions() -> Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let library = root.path().join("src/lib.rs");
    let text = "pub fn ordinary() {}\npub fn caller() { ordinary(); ordinary(); }\n";
    std::fs::write(&library, text).into_diagnostic()?;
    let state = tempfile::tempdir().into_diagnostic()?;
    let (_, before) = run(root.path(), &SavedState::at(state.path().join("one")))?;
    let (before_all, before) = (
        evidence_by_label(&before, false)?,
        evidence_by_label(&before, true)?,
    );

    std::fs::write(&library, format!("// a comment above everything\n{text}")).into_diagnostic()?;
    let (_, after) = run(root.path(), &SavedState::at(state.path().join("two")))?;
    let (after_all, after) = (
        evidence_by_label(&after, false)?,
        evidence_by_label(&after, true)?,
    );
    assert_ne!(
        before_all, after_all,
        "the comment shifted the ranges, which is what this test is about"
    );

    assert_eq!(
        before.keys().collect::<Vec<_>>(),
        after.keys().collect::<Vec<_>>()
    );
    let mut moved = Vec::new();
    for (label, evidence) in &before {
        let (Some(old), Some(new)) = (evidence.as_object(), after[label].as_object()) else {
            continue;
        };
        for (field, value) in old {
            if new.get(field) != Some(value) {
                moved.push(format!("{label}.{field}: {value} -> {}", new[field]));
            }
        }
    }
    assert!(
        moved.is_empty(),
        "a comment changed the evidence: {moved:#?}"
    );
    Ok(())
}

//! A cfg context's crates and file copies are saved with the database and taken up again by
//! the next run, so what was computed for them is not computed again.

mod support {
    pub mod executions;
    pub mod semantic_metrics;
}

use cordial::{
    CfgPlanning, CrateIr, CrateTarget, IrView, SavedState, SemanticCargoOptions,
    SemanticContextKey, SourceDatabase,
};
use miette::{IntoDiagnostic, Result};
use std::path::Path;
use support::executions::Executions;
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

/// The node of the function named `name`, which is the only one.
fn node_named(graph: &CrateIr, name: &str) -> Result<cordial::NodeId> {
    let mut found = graph
        .nodes_matching(&cordial::BasicQuery::items())
        .into_iter()
        .filter(|node| node.weight().name().as_deref() == Some(name))
        .map(|node| node.id());
    match (found.next(), found.next()) {
        (Some(id), None) => Ok(id),
        other => Err(miette::miette!("{name}: {other:?}")),
    }
}

/// A function a context's copy declares is the ordinary build's function as far as facts go, in a
/// run that took the context up from a saved state as in the run that made it: what it calls is
/// named as the ordinary build names it, not by the copy's identity nothing else knows.
#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_restored_contexts_facts_name_the_ordinary_functions_they_call() -> Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = SavedState::at(state.path().join("analysis-state"));
    run(root.path(), &saved)?;
    // The edit makes the restored run harvest the context again.
    std::fs::write(
        root.path().join("src/lib.rs"),
        format!("{LIBRARY}\n#[cfg(kani)]\npub fn second_harness() {{ ordinary(); }}\n"),
    )
    .into_diagnostic()?;
    let (restored, graph) = run(root.path(), &saved)?;
    assert!(restored);
    let ordinary = graph
        .semantic_function(node_named(&graph, "ordinary")?)
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("ordinary has facts"))?;
    let entries = graph
        .semantic_context_functions(node_named(&graph, "second_harness")?)
        .into_diagnostic()?;
    let call = entries
        .first()
        .and_then(|entry| entry.function().calls().first())
        .ok_or_else(|| miette::miette!("the new harness calls ordinary"))?;
    assert_eq!(
        call.target(),
        &cordial::SemanticCallTarget::Function(*ordinary.definition()),
        "the restored context's call names the copy's identity, which nothing else knows"
    );
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
    const NAMING: [&str; 17] = [
        "local_functions",
        "derived_site",
        "inner_",
        "declared_in",
        "resolve",
        "harvest",
        "written_in_file",
        "generated_in",
        // The places of a file's items, indexed once from its parse.
        "places_in",
        // What the ordinary build's crates declare, listed once for all the contexts.
        "places_declared_in",
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

/// The interned ids in `value` (a function's `definition` and the function a call's `target`
/// names) follow the order a database happened to evaluate in, and mean nothing across two.
fn without_ids(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            map.remove("definition");
            if let Some(serde_json::Value::Object(target)) = map.get_mut("target")
                && let Some(id) = target.get_mut("Function")
            {
                *id = serde_json::Value::from(0);
            }
            map.values_mut().for_each(without_ids);
        }
        serde_json::Value::Array(items) => items.iter_mut().for_each(without_ids),
        _ => {}
    }
}

/// Each function's evidence as JSON, by label, without ids and optionally without the source
/// ranges of its calls.
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
            without_ids(&mut value);
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

/// Two functions in one file; an edit to the text above them, in a process that has already
/// evaluated them. Returns how many functions were evaluated again.
fn evaluated_after_editing(edit: impl FnOnce(&str) -> String) -> Result<usize> {
    let root = fixture()?;
    let library = root.path().join("src/lib.rs");
    let text = "pub fn ordinary() {}\npub fn caller() { ordinary(); }\n";
    std::fs::write(&library, text).into_diagnostic()?;
    let mut session = SourceDatabase::load(root.path()).into_diagnostic()?;
    let target = CrateTarget::new("gated", root.path());
    session
        .project(&target, std::slice::from_ref(&library))
        .into_diagnostic()?;
    workspace_evidence()
        .ok_or_else(|| miette::miette!("the first projection logs its evidence"))?;

    session
        .change_file(&library, edit(text))
        .into_diagnostic()?;
    session.project(&target, &[library]).into_diagnostic()?;
    let (evaluated, _) = workspace_evidence()
        .ok_or_else(|| miette::miette!("the second projection logs its evidence"))?;
    Ok(evaluated)
}

/// The same edit, a comment above both functions, with its result asserted both ways: a comment
/// leaves what each function says unchanged (the ranges aside), so it should evaluate nothing;
/// a change to a function's body must evaluate that function.
#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn an_edit_evaluates_only_the_functions_it_changes_in_the_process_that_made_it() -> Result<()> {
    cordial::init_tracing();
    let body = evaluated_after_editing(|text| {
        text.replace("{ ordinary(); }", "{ ordinary(); ordinary(); }")
    })?;
    assert!(
        body >= 1,
        "an edit to a body evaluates that function ({body})"
    );

    let comment = evaluated_after_editing(|text| format!("// a comment\n{text}"))?;
    assert_eq!(
        comment, 0,
        "a comment evaluated {comment} functions again in the same process"
    );
    Ok(())
}

/// A crate that depends on `count` crates of one function each.
fn crate_with_dependencies(count: usize) -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir_all(root.path().join("src")).into_diagnostic()?;
    let mut manifest =
        String::from("[package]\nname='gated'\nversion='0.1.0'\nedition='2024'\n[dependencies]\n");
    let mut library = String::from("pub fn top() {\n");
    for n in 0..count {
        let directory = root.path().join(format!("dep{n}"));
        std::fs::create_dir_all(directory.join("src")).into_diagnostic()?;
        std::fs::write(
            directory.join("Cargo.toml"),
            format!("[package]\nname='dep{n}'\nversion='0.1.0'\nedition='2024'\n"),
        )
        .into_diagnostic()?;
        std::fs::write(directory.join("src/lib.rs"), "pub fn leaf() {}\n").into_diagnostic()?;
        manifest.push_str(&format!("dep{n} = {{ path = \"dep{n}\" }}\n"));
        library.push_str(&format!("    dep{n}::leaf();\n"));
    }
    manifest.push_str("[workspace]\n");
    library.push_str("}\n");
    std::fs::write(root.path().join("Cargo.toml"), manifest).into_diagnostic()?;
    std::fs::write(root.path().join("src/lib.rs"), library).into_diagnostic()?;
    Ok(root)
}

/// The queries a cold and then a restored run executed over a crate with `count` dependencies.
fn executions_over(count: usize) -> Result<(usize, usize)> {
    use tracing_subscriber::prelude::*;
    let executions = Executions::default();
    let subscriber = tracing_subscriber::registry().with(executions.clone());
    let _subscriber = tracing::subscriber::set_default(subscriber);
    let root = crate_with_dependencies(count)?;
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = SavedState::at(state.path().join("analysis-state"));
    run(root.path(), &saved)?;
    let cold = executions.take().values().sum();
    run(root.path(), &saved)?;
    Ok((cold, executions.take().values().sum()))
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn the_queries_a_run_executes_grow_with_the_crates_in_it() -> Result<()> {
    cordial::init_tracing();
    let (_, few) = executions_over(2)?;
    let (_, many) = executions_over(32)?;
    // Measured: about 14 queries for each crate added, over a fixed cost of the sysroot that
    // does not depend on the workspace (about 27,000). A run that did work for each crate in
    // proportion to its size would be well above this.
    let per_crate = many.saturating_sub(few) / 30;
    assert!(
        per_crate < 100,
        "a restored run executes {per_crate} queries for each crate added ({few} for 2, {many} for 32)"
    );
    Ok(())
}

/// A crate whose two test binaries both include `tests/support/mod.rs`, so that file's
/// functions are defined once in each of two crates.
fn shared_support_file() -> Result<tempfile::TempDir> {
    let root = fixture()?;
    std::fs::create_dir_all(root.path().join("tests/support")).into_diagnostic()?;
    std::fs::write(
        root.path().join("tests/support/mod.rs"),
        "pub fn helper() {}\npub fn other_helper() { helper(); }\n\
         pub fn with_local_impl() { struct Local; impl Local { fn inside(&self) {} } Local.inside(); }\n",
    )
    .into_diagnostic()?;
    for name in ["first", "second"] {
        std::fs::write(
            root.path().join(format!("tests/{name}.rs")),
            "mod support;\n#[test]\nfn uses_support() { support::other_helper(); }\n\
             trait Layer { fn on_event(&self); }\nstruct Reports;\n\
             impl Layer for Reports {\n    fn on_event(&self) {\n        struct Fields;\n\
                 trait Visit { fn record(&self); }\n\
                 impl Visit for Fields { fn record(&self) {} }\n        Fields.record();\n    }\n}\n",
        )
        .into_diagnostic()?;
    }
    Ok(root)
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_file_two_crates_include_is_found_again_for_each_of_them() -> Result<()> {
    cordial::init_tracing();
    let root = shared_support_file()?;
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = SavedState::at(state.path().join("analysis-state"));
    run(root.path(), &saved)?;
    let (cold, total) =
        workspace_evidence().ok_or_else(|| miette::miette!("the cold run logs its evidence"))?;
    assert_eq!(cold, total);

    let (restored, _) = run(root.path(), &saved)?;
    assert!(restored);
    let (again, total) = workspace_evidence()
        .ok_or_else(|| miette::miette!("the restored run logs its evidence"))?;
    assert_eq!(
        again, 0,
        "{again} of {total} functions were evaluated again; the shared file's are one each"
    );
    Ok(())
}

/// A crate with one file of `count` impl blocks of one method each, with its state saved by a
/// first run, and how long a second, restored run takes.
fn restored_time_over_impls(count: usize) -> Result<std::time::Duration> {
    let root = fixture()?;
    let mut text = String::from("pub struct S;\n");
    for n in 0..count {
        text.push_str(&format!(
            "impl S {{ pub fn m{n}(&self) -> usize {{ {n} }} }}\n"
        ));
    }
    std::fs::write(root.path().join("src/lib.rs"), text).into_diagnostic()?;
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = SavedState::at(state.path().join("analysis-state"));
    run(root.path(), &saved)?;
    let started = std::time::Instant::now();
    run(root.path(), &saved)?;
    Ok(started.elapsed())
}

/// Naming a function by where it is must not take time in proportion to the square of the items
/// in its file: one file of many impl blocks with one label (so each is told from the others by
/// an ordinal) took 68 s to restore at 4,000 impls when each was counted against the ones before
/// it, and takes 12 s now, over a base of about 4 s.
#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn the_time_to_name_functions_does_not_grow_with_the_square_of_a_files_items() -> Result<()> {
    cordial::init_tracing();
    let few = restored_time_over_impls(1000)?;
    let many = restored_time_over_impls(3000)?;
    // Three times the items: linear is about three times the work above the base; the base is
    // most of the small run. Counting siblings one by one made it nine times.
    assert!(
        many < few * 5,
        "3,000 impls took {many:?} against {few:?} for 1,000: more than linear growth"
    );
    Ok(())
}

/// How many times a span of the given name was entered, from its `new_span` events.
#[derive(Default, Clone)]
struct SpanCount(std::sync::Arc<std::sync::atomic::AtomicUsize>, &'static str);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for SpanCount {
    fn on_new_span(
        &self,
        attributes: &tracing::span::Attributes<'_>,
        _: &tracing::span::Id,
        _: tracing_subscriber::layer::Context<'_, S>,
    ) {
        if attributes.metadata().name() == self.1 {
            self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }
}

/// The ordinary build's declarations are the same for every context, so they are listed once for
/// each crate, and a context's copies are not listed at all: a copy's counterpart is found from
/// its own place when it is asked for. Three crates and three contexts is 3 listings and the two
/// the index of generated definitions asks for, where listing every copy again for each context
/// was 3 + 3 x 3.
#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn the_ordinary_builds_declarations_are_listed_once_for_all_the_contexts() -> Result<()> {
    cordial::init_tracing();
    let count = SpanCount(Default::default(), "declared_in");
    let subscriber = tracing_subscriber::registry().with(count.clone());
    let _subscriber = tracing::subscriber::set_default(subscriber);
    let root = chain_of_crates()?;
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = SavedState::at(state.path().join("analysis-state"));
    run_many(root.path(), &saved)?;
    let listed = count.0.load(std::sync::atomic::Ordering::Relaxed);
    let (crates, contexts, others) = (3, 3, 2);
    assert!(
        listed <= crates + others,
        "{listed} listings of what a crate declares for {crates} crates and {contexts} contexts"
    );
    Ok(())
}

/// A file's places are indexed once however many items ask for them: the index is a query whose
/// answer is kept, so it is built once for each file and not for each item in it.
#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_files_places_are_indexed_once_for_all_the_items_that_ask() -> Result<()> {
    cordial::init_tracing();
    let executions = Executions::default();
    let subscriber = tracing_subscriber::registry().with(executions.clone());
    let _subscriber = tracing::subscriber::set_default(subscriber);
    let root = chain_of_crates()?;
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = SavedState::at(state.path().join("analysis-state"));
    run_many(root.path(), &saved)?;
    let built: usize = executions
        .take()
        .into_iter()
        .filter(|(query, _)| query.ends_with("> places_in"))
        .map(|(_, count)| count)
        .sum();
    // Three crates of one file each, and three contexts that copy each.
    let (files, contexts) = (3, 3);
    assert!(
        (1..=files * (1 + contexts)).contains(&built),
        "{built} indexes executed for {files} files and {contexts} contexts"
    );
    Ok(())
}

/// The files walked for their places in a run of the workspace over `contexts`.
fn walks_over(contexts: Vec<SemanticContextKey>) -> Result<usize> {
    let count = SpanCount(Default::default(), "place_index");
    let subscriber = tracing_subscriber::registry().with(count.clone());
    let _subscriber = tracing::subscriber::set_default(subscriber);
    let root = chain_of_crates()?;
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = SavedState::at(state.path().join("analysis-state"));
    let options = SemanticCargoOptions::default()
        .with_cfg_contexts(contexts)
        .with_cfg_planning(CfgPlanning::Explicit);
    let mut session =
        SourceDatabase::load_saved(root.path(), &options, Some(saved)).into_diagnostic()?;
    session
        .project(
            &CrateTarget::new("gated", root.path()),
            &[root.path().join("src/lib.rs")],
        )
        .into_diagnostic()?;
    Ok(count.0.load(std::sync::atomic::Ordering::Relaxed))
}

/// A context's copy of a file has the text of its original, so the file is parsed and walked for
/// its places once, not once more for each context that copies it: three contexts walk what none
/// does.
#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_files_copies_are_not_walked_again_for_their_places() -> Result<()> {
    cordial::init_tracing();
    let alone = walks_over(Vec::new())?;
    let contexts = vec![
        SemanticContextKey::enabling(["kani"]),
        SemanticContextKey::enabling(["creusot"]),
        SemanticContextKey::enabling(["kani", "creusot"]),
    ];
    let with = walks_over(contexts)?;
    assert!(alone > 0, "the workspace's own files are walked");
    assert_eq!(with, alone, "the contexts' copies were walked again");
    Ok(())
}

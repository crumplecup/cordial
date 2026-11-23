//! Which step of the tool asks for what while the evidence of a workspace's functions is computed:
//! the queries Salsa executes, by the step that asked, for a workspace of a known number of functions.

use cordial::{
    CrateTarget, MetricsLayer, SemanticCargoOptions, SourceDatabase, StoreLayout,
    metrics_filter_with,
};
use miette::{IntoDiagnostic, Result};
use tracing_subscriber::{Layer, layer::SubscriberExt};

/// Two sizes of the same workspace: what grows between them is what a function costs, and what
/// does not is what the libraries cost once.
const SMALL: usize = 20;
const LARGE: usize = 60;

fn source(functions: usize) -> String {
    let mut text = String::from(
        "#[derive(Debug, Clone, PartialEq)]\npub struct Item { pub n: u32 }\n\
         impl Item { pub fn get(&self) -> u32 { self.n } }\n",
    );
    for index in 0..functions {
        let next = (index + 1) % functions;
        text.push_str(&format!(
            "pub fn f{index}(items: &[Item]) -> u32 {{\n\
             \x20   let total: u32 = items.iter().map(|item| item.get()).sum();\n\
             \x20   if total > {index} {{ fn inner() -> u32 {{ 1 }} total + inner() }} else {{ f{next}(items) }}\n\
             }}\n"
        ));
    }
    text
}

fn fixture(functions: usize) -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir_all(root.path().join("src")).into_diagnostic()?;
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='app'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    std::fs::write(root.path().join("src/lib.rs"), source(functions)).into_diagnostic()?;
    Ok(root)
}

/// The queries Salsa has executed so far, by the step that asked and the query, from the run's metrics.
fn executed(root: &std::path::Path) -> Result<std::collections::BTreeMap<String, u64>> {
    let store = StoreLayout::from_root(root.join("store"), "specimen".to_owned());
    cordial::write_run_metrics(&store).into_diagnostic()?;
    let metrics = cordial::read_run_metrics(&store);
    Ok(metrics
        .values()
        .iter()
        .filter(|(name, _)| name.starts_with("steps."))
        .map(|(name, count)| (name.trim_start_matches("steps.").to_owned(), *count))
        .collect())
}

fn project(functions: usize) -> Result<tempfile::TempDir> {
    let root = fixture(functions)?;
    let mut session = SourceDatabase::load_with(root.path(), &SemanticCargoOptions::default())
        .into_diagnostic()?;
    session
        .project(
            &CrateTarget::new("app", root.path()),
            &[root.path().join("src/lib.rs")],
        )
        .into_diagnostic()?;
    Ok(root)
}

/// What a function costs, and not what the libraries cost once: the queries Salsa executes for each
/// function added to a workspace, by the step that asked. Evaluating a function's evidence asks for
/// its body, its inference and what it reaches a small number of times each; a step that asked for
/// more than that for each function added is the step to look at, and this says so before a run of
/// thousands of functions does.
#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_function_added_costs_a_few_queries_of_each_kind_and_no_step_asks_for_more_for_each()
-> Result<()> {
    cordial::init_tracing();
    let subscriber =
        tracing_subscriber::registry().with(MetricsLayer.with_filter(metrics_filter_with(true)));
    let (small, large) = tracing::subscriber::with_default(subscriber, || -> Result<_> {
        let root = project(SMALL)?;
        let small = executed(root.path())?;
        let root = project(LARGE)?;
        let all = executed(root.path())?;
        let large: std::collections::BTreeMap<String, u64> = all
            .iter()
            .map(|(name, count)| (name.clone(), count - small.get(name).copied().unwrap_or(0)))
            .collect();
        Ok((small, large))
    })?;
    let added = (LARGE - SMALL) as f64;
    let per_function: Vec<(f64, &String)> = large
        .iter()
        .map(|(name, big)| {
            let little = small.get(name).copied().unwrap_or(0);
            ((*big as f64 - little as f64) / added, name)
        })
        .collect();

    // No kind of query for any step grows by more than two for each function added (2.0 is what it is).
    let (worst, name) = per_function
        .iter()
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(per, name)| (*per, name.as_str()))
        .unwrap_or((0.0, ""));
    assert!(
        worst <= 2.5,
        "{name} asks {worst:.2} times for each function added"
    );
    // And the whole of it is a bounded number of queries for each function (29 today): the sum over every kind
    // and step, so that many kinds each asking a little cannot hide.
    let total: f64 = per_function.iter().map(|(per, _)| per.max(0.0)).sum();
    assert!(
        total <= 32.0,
        "{total:.1} queries for each function added: {:?}",
        {
            let mut top: Vec<_> = per_function.iter().filter(|(per, _)| *per >= 1.0).collect();
            top.sort_by(|a, b| b.0.total_cmp(&a.0));
            top.into_iter()
                .take(10)
                .map(|(per, name)| format!("{per:.1} {name}"))
                .collect::<Vec<_>>()
        }
    );
    Ok(())
}

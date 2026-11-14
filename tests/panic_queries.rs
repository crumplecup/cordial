#![cfg(feature = "panics")]
//! Panic evidence is queries: a file's scan reads the set of functions the Kani
//! proofs reach, so an edit that leaves the proof graph equal re-scans only the
//! edited file, and one that changes what the proofs reach re-scans every file.

mod support {
    pub mod semantic_metrics;
}

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use cordial::{CrateTarget, SourceDatabase};
use miette::{IntoDiagnostic, Result};
use tracing_subscriber::prelude::*;

const FILES: [(&str, &str); 3] = [
    ("lib.rs", "mod a;\nmod b;\n"),
    (
        "a.rs",
        "pub fn work(x: Option<u32>) -> u32 { x.unwrap() }\n",
    ),
    ("b.rs", "pub fn helper() {}\n"),
];

fn counts(
    metrics: &support::semantic_metrics::QueryMetrics,
    query: &str,
) -> BTreeMap<String, usize> {
    metrics
        .snapshot()
        .into_iter()
        .filter(|(key, _)| key.starts_with(query))
        .collect()
}

fn project(session: &mut SourceDatabase, root: &Path) -> Result<()> {
    let paths: Vec<PathBuf> = FILES
        .iter()
        .map(|(file, _)| root.join("src").join(file))
        .collect();
    session
        .project(&CrateTarget::new("panic_specimen", root), &paths)
        .into_diagnostic()?;
    Ok(())
}

fn total(metrics: &support::semantic_metrics::QueryMetrics, query: &str) -> usize {
    counts(metrics, query).values().sum()
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn an_edit_that_leaves_the_proofs_equal_rescans_only_its_file() -> Result<()> {
    cordial::init_tracing();
    let metrics = support::semantic_metrics::QueryMetrics::default();
    let subscriber = tracing_subscriber::registry()
        .with(metrics.clone())
        .with(cordial::tracing_fmt_layer());
    let _subscriber = tracing::subscriber::set_default(subscriber);
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir(root.path().join("src")).into_diagnostic()?;
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname = \"panic_specimen\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[workspace]\n",
    )
    .into_diagnostic()?;
    for (file, text) in FILES {
        std::fs::write(root.path().join("src").join(file), text).into_diagnostic()?;
    }
    let mut session = SourceDatabase::load(root.path()).into_diagnostic()?;
    project(&mut session, root.path())?;
    let scans = counts(&metrics, "panic_scan_file:");
    assert_eq!(scans.len(), 3, "every file is scanned once: {scans:?}");
    assert!(scans.values().all(|count| *count == 1), "{scans:?}");
    assert_eq!(total(&metrics, "panic_graph:"), 1);

    // Another unwrap changes no function or call: the file's graph comes out
    // equal, so the join stops there and only the edited file is scanned again.
    let a = root.path().join("src/a.rs");
    session
        .change_file(
            &a,
            "pub fn work(x: Option<u32>) -> u32 { x.unwrap() + x.unwrap() }\n".to_owned(),
        )
        .into_diagnostic()?;
    project(&mut session, root.path())?;
    let after = counts(&metrics, "panic_scan_file:");
    for (key, count) in &after {
        let was = scans.get(key).copied().unwrap_or(0);
        if key.ends_with("a.rs") {
            assert_eq!(
                *count,
                was + 1,
                "the edited file was not scanned again: {key}"
            );
        } else {
            assert_eq!(*count, was, "another file was scanned again: {key}");
        }
    }
    assert_eq!(
        total(&metrics, "panic_graph:"),
        1,
        "an equal proof graph leaves the join alone"
    );

    // A proof that reaches `helper` changes what every file's scan may exempt.
    let b = root.path().join("src/b.rs");
    session
        .change_file(
            &b,
            "pub fn helper() {}\n#[kani::proof]\nfn check() { helper(); }\n".to_owned(),
        )
        .into_diagnostic()?;
    project(&mut session, root.path())?;
    assert_eq!(total(&metrics, "panic_graph:"), 2);
    assert_eq!(total(&metrics, "panic_reachable:"), 2);
    for (key, count) in &counts(&metrics, "panic_scan_file:") {
        assert!(
            *count > after.get(key).copied().unwrap_or(0),
            "a changed reachable set left a scan unrun: {key}"
        );
    }
    Ok(())
}

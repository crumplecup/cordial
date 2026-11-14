#![cfg(feature = "antipatterns")]
//! Antipattern evidence is queries: a file's walk reads the crate's joined facts,
//! so an edit that leaves those facts equal re-walks only the edited file, and one
//! that changes them re-walks the files the change can reach.

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
    ("a.rs", "pub fn quiet() -> u32 { 1 }\n"),
    (
        "b.rs",
        "#[derive(Debug)]\npub struct Fail;\nimpl std::fmt::Display for Fail {\n    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, \"fail\") }\n}\nimpl std::error::Error for Fail {}\n",
    ),
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
        .project(&CrateTarget::new("antipattern_specimen", root), &paths)
        .into_diagnostic()?;
    Ok(())
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn an_edit_that_leaves_the_facts_equal_rewalks_only_its_file() -> Result<()> {
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
        "[package]\nname = \"antipattern_specimen\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[workspace]\n",
    )
    .into_diagnostic()?;
    for (file, text) in FILES {
        std::fs::write(root.path().join("src").join(file), text).into_diagnostic()?;
    }
    let mut session = SourceDatabase::load(root.path()).into_diagnostic()?;
    project(&mut session, root.path())?;
    let facts_before = counts(&metrics, "antipattern_facts_file:");
    let walks = counts(&metrics, "antipattern_sites_file:");
    assert_eq!(walks.len(), 3, "every file is walked once: {walks:?}");
    assert!(walks.values().all(|count| *count == 1), "{walks:?}");
    let joins = counts(&metrics, "antipattern_crate_facts:")
        .values()
        .sum::<usize>();
    assert_eq!(joins, 1);

    // The body of a function changes no fact: the file's facts come out equal, so
    // the join does not run at all, and only the edited file is walked again.
    let a = root.path().join("src/a.rs");
    session
        .change_file(&a, "pub fn quiet() -> u32 { 2 }\n".to_owned())
        .into_diagnostic()?;
    project(&mut session, root.path())?;
    for (key, count) in &counts(&metrics, "antipattern_facts_file:") {
        let was = facts_before.get(key).copied().unwrap_or(0);
        if key.ends_with("a.rs") {
            assert_eq!(
                *count,
                was + 1,
                "the edited file's facts were reused: {key}"
            );
        } else {
            assert_eq!(*count, was, "another file's facts were recomputed: {key}");
        }
    }
    let after = counts(&metrics, "antipattern_sites_file:");
    for (key, count) in &after {
        let was = walks.get(key).copied().unwrap_or(0);
        if key.ends_with("a.rs") {
            assert_eq!(
                *count,
                was + 1,
                "the edited file was not walked again: {key}"
            );
        } else {
            assert_eq!(*count, was, "another file was walked again: {key}");
        }
    }
    assert_eq!(
        counts(&metrics, "antipattern_crate_facts:")
            .values()
            .sum::<usize>(),
        joins,
        "equal facts from the edited file leave the join alone"
    );

    // A new error type changes the facts every file's walk reads.
    session
        .change_file(
            &a,
            "#[derive(Debug)]\npub struct Added;\nimpl std::fmt::Display for Added {\n    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, \"a\") }\n}\nimpl std::error::Error for Added {}\n".to_owned(),
        )
        .into_diagnostic()?;
    project(&mut session, root.path())?;
    assert_eq!(
        counts(&metrics, "antipattern_crate_facts:")
            .values()
            .sum::<usize>(),
        joins + 1,
        "a new error type is a new fact, so the facts are joined again"
    );
    let changed = counts(&metrics, "antipattern_sites_file:");
    for (key, count) in &changed {
        assert!(
            *count > after.get(key).copied().unwrap_or(0),
            "a changed fact left a walk unrun: {key}"
        );
    }
    Ok(())
}

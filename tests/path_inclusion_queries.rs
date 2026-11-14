#![cfg(feature = "derives")]
//! A file's `#[path]` targets are a query keyed by the file and its package: asked
//! again nothing runs, and a body edit recomputes the edited file's targets only.

mod support {
    pub mod semantic_metrics;
}

use std::collections::BTreeMap;

use cordial::SourceDatabase;
use miette::{IntoDiagnostic, Result};
use tracing_subscriber::prelude::*;

const FILES: [(&str, &str); 3] = [
    ("lib.rs", "mod a;\n#[path = \"other.rs\"]\nmod b;\n"),
    ("a.rs", "pub fn quiet() -> u32 { 1 }\n"),
    ("other.rs", "pub fn other() -> u32 { 1 }\n"),
];

fn counts(metrics: &support::semantic_metrics::QueryMetrics) -> BTreeMap<String, usize> {
    metrics
        .snapshot()
        .into_iter()
        .filter(|(key, _)| key.starts_with("path_inclusion_file:"))
        .collect()
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_body_edit_recomputes_only_its_files_targets() -> Result<()> {
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
        "[package]\nname = \"inclusion_specimen\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[workspace]\n",
    )
    .into_diagnostic()?;
    for (file, text) in FILES {
        std::fs::write(root.path().join("src").join(file), text).into_diagnostic()?;
    }
    let mut session = SourceDatabase::load(root.path()).into_diagnostic()?;
    let first = session.project_path_inclusions().into_diagnostic()?;
    let ran = counts(&metrics);
    assert_eq!(ran.len(), 3, "every file once: {ran:?}");
    assert!(ran.values().all(|count| *count == 1), "{ran:?}");
    assert!(
        first
            .sources()
            .iter()
            .any(|source| source.targets().iter().any(|target| target == "other.rs")),
        "lib.rs declares the target other.rs: {first:?}"
    );

    assert_eq!(session.project_path_inclusions().into_diagnostic()?, first);
    assert_eq!(counts(&metrics), ran, "asked again, nothing runs");

    session
        .change_file(
            &root.path().join("src/a.rs"),
            "pub fn quiet() -> u32 { 2 }\n".to_owned(),
        )
        .into_diagnostic()?;
    session.project_path_inclusions().into_diagnostic()?;
    for (key, count) in &counts(&metrics) {
        let was = ran.get(key).copied().unwrap_or(0);
        if key.ends_with("a.rs") {
            assert_eq!(*count, was + 1, "the edited file was reused: {key}");
        } else {
            assert_eq!(*count, was, "another file was recomputed: {key}");
        }
    }
    Ok(())
}

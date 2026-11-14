#![cfg(feature = "internal_error_chain")]
//! The error architecture catalog and the signature index are queries: each file's
//! part is keyed by the file and the crate's value is joined from the parts, so an
//! edit recomputes the edited file's part and no other file's.

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
        "#[derive(Debug)]\npub struct Fail;\nimpl std::fmt::Display for Fail {\n    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(\"fail\") }\n}\nimpl std::error::Error for Fail {}\n",
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
        .project(&CrateTarget::new("evidence_specimen", root), &paths)
        .into_diagnostic()?;
    Ok(())
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn an_edit_recomputes_only_its_files_architecture_and_signature_parts() -> Result<()> {
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
        "[package]\nname = \"evidence_specimen\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[workspace]\n",
    )
    .into_diagnostic()?;
    for (file, text) in FILES {
        std::fs::write(root.path().join("src").join(file), text).into_diagnostic()?;
    }
    let mut session = SourceDatabase::load(root.path()).into_diagnostic()?;
    project(&mut session, root.path())?;
    for query in ["architecture_file:", "signature_file:"] {
        let parts = counts(&metrics, query);
        assert_eq!(parts.len(), 3, "{query} runs for every file: {parts:?}");
        assert!(parts.values().all(|count| *count == 1), "{parts:?}");
    }
    for query in ["architecture_catalog:", "signature_index:"] {
        assert_eq!(
            counts(&metrics, query).values().sum::<usize>(),
            1,
            "{query}"
        );
    }
    let before: Vec<_> = ["architecture_file:", "signature_file:"]
        .map(|query| counts(&metrics, query))
        .into();

    // A new function in one file is recomputed there and in no other file.
    let a = root.path().join("src/a.rs");
    session
        .change_file(&a, "pub fn quiet() -> u32 { 2 }\n".to_owned())
        .into_diagnostic()?;
    project(&mut session, root.path())?;
    for (query, was) in ["architecture_file:", "signature_file:"]
        .iter()
        .zip(&before)
    {
        for (key, count) in &counts(&metrics, query) {
            let earlier = was.get(key).copied().unwrap_or(0);
            if key.ends_with("a.rs") {
                assert_eq!(*count, earlier + 1, "{query} reused the edited file: {key}");
            } else {
                assert_eq!(*count, earlier, "{query} recomputed another file: {key}");
            }
        }
    }
    // The signature index gained a function, so it is joined again.
    assert_eq!(
        counts(&metrics, "signature_index:").values().sum::<usize>(),
        2
    );
    Ok(())
}

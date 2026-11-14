#![cfg(feature = "internal_error_chain")]
//! Apply's crate-wide evidence is read through queries over the per-file results:
//! asked twice it is computed once, and an edit recomputes only the edited file's
//! facts.

#[path = "support/semantic_metrics.rs"]
mod semantic_metrics;

use std::collections::BTreeMap;
use std::fs;

use cordial::SourceDatabase;
use miette::IntoDiagnostic;
use tracing_subscriber::layer::SubscriberExt;

const LIB: &str = "mod other;\n#[derive(Debug)]\nstruct Wrapper { message: String }\nimpl std::fmt::Display for Wrapper {\n    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, \"{}\", self.message) }\n}\nimpl std::error::Error for Wrapper {}\nfn make() -> Wrapper { Wrapper { message: \"fixed\".to_owned() } }\n";
const OTHER: &str = "pub fn helper() -> usize { 1 }\n";

fn counts(metrics: &semantic_metrics::QueryMetrics, query: &str) -> BTreeMap<String, usize> {
    metrics
        .snapshot()
        .into_iter()
        .filter(|(key, _)| key.starts_with(query))
        .collect()
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn apply_evidence_is_computed_once_and_an_edit_recomputes_only_its_file() -> miette::Result<()> {
    cordial::init_tracing();
    let metrics = semantic_metrics::QueryMetrics::default();
    let subscriber = tracing_subscriber::registry()
        .with(metrics.clone())
        .with(cordial::tracing_fmt_layer());
    let _subscriber = tracing::subscriber::set_default(subscriber);
    let root = tempfile::tempdir().into_diagnostic()?;
    fs::create_dir_all(root.path().join("src")).into_diagnostic()?;
    fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname = \"apply-evidence-queries\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .into_diagnostic()?;
    fs::write(root.path().join("src/lib.rs"), LIB).into_diagnostic()?;
    fs::write(root.path().join("src/other.rs"), OTHER).into_diagnostic()?;
    let mut database = SourceDatabase::load(root.path()).into_diagnostic()?;

    let first = database.conversion_units(root.path()).into_diagnostic()?;
    assert!(
        first.iter().any(|unit| unit.field_name() == "message"),
        "the unit of Wrapper.message: {first:?}"
    );
    let ran = |name: &str| counts(&metrics, name).values().sum::<usize>();
    let (source, index, units) = (
        ran("apply_source:"),
        ran("apply_index:"),
        ran("apply_units:"),
    );
    assert_eq!((source, index, units), (1, 1, 1));
    let per_file = counts(&metrics, "error_apply_file:");
    assert_eq!(per_file.len(), 2, "one per file: {per_file:?}");

    // Asked again, nothing runs.
    assert_eq!(
        database.conversion_units(root.path()).into_diagnostic()?,
        first
    );
    assert_eq!(counts(&metrics, "error_apply_file:"), per_file);
    assert_eq!(ran("apply_source:"), source);
    assert_eq!(ran("apply_units:"), units);

    // An edit to one file recomputes that file's facts and the crate-wide phases
    // above them, and no other file's facts.
    let other = root.path().join("src/other.rs");
    database
        .change_file(&other, "pub fn helper() -> usize { 2 }\n".to_owned())
        .into_diagnostic()?;
    assert_eq!(
        database.conversion_units(root.path()).into_diagnostic()?,
        first
    );
    let after = counts(&metrics, "error_apply_file:");
    for (key, count) in &after {
        let was = per_file.get(key).copied().unwrap_or(0);
        if key.ends_with("other.rs") {
            assert_eq!(
                *count,
                was + 1,
                "the edited file's facts were reused: {key}"
            );
        } else {
            assert_eq!(*count, was, "another file's facts were recomputed: {key}");
        }
    }
    assert_eq!(ran("apply_source:"), source + 1);
    assert_eq!(ran("apply_index:"), index + 1);
    Ok(())
}

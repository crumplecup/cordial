//! A restored run reuses what was computed for a crate that uses a procedural macro: the
//! macro's expander is a new object each process, but the macro is the same.

mod support {
    pub mod semantic_metrics;
}

use cordial::{CrateTarget, SavedState, SemanticCargoOptions, SourceDatabase};
use miette::{IntoDiagnostic, Result};
use std::path::Path;
use tracing_subscriber::prelude::*;

/// A derive that adds a function, so that expanding it matters.
const MACROS: &str = r#"
extern crate proc_macro;
use proc_macro::TokenStream;

#[proc_macro_derive(Hello)]
pub fn hello(_input: TokenStream) -> TokenStream {
    "pub fn hello_generated() {}".parse().expect("valid items")
}
"#;

const USER: &str = "\
#[derive(macros::Hello)]
pub struct Widget;

pub fn caller() -> Widget {
    Widget
}
";

fn specimen() -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    for directory in ["src", "macros/src"] {
        std::fs::create_dir_all(root.path().join(directory)).into_diagnostic()?;
    }
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname = \"user\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\
         [dependencies]\nmacros = { path = \"macros\" }\n[workspace]\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        root.path().join("macros/Cargo.toml"),
        "[package]\nname = \"macros\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\
         [lib]\nproc-macro = true\n",
    )
    .into_diagnostic()?;
    std::fs::write(root.path().join("macros/src/lib.rs"), MACROS).into_diagnostic()?;
    std::fs::write(root.path().join("src/lib.rs"), USER).into_diagnostic()?;
    Ok(root)
}

fn run(root: &Path, saved: &SavedState) -> Result<bool> {
    let mut session =
        SourceDatabase::load_saved(root, &SemanticCargoOptions::default(), Some(saved.clone()))
            .into_diagnostic()?;
    let restored = session.restored();
    session
        .project(&CrateTarget::new("user", root), &[root.join("src/lib.rs")])
        .into_diagnostic()?;
    Ok(restored)
}

/// What a restored run must not compute: the per-file queries the projection runs.
const PER_FILE: [&str; 6] = [
    "file_facts:",
    "file_items:",
    "visibility_file:",
    "policy_file:",
    "error_sites_file:",
    "antipattern_facts_file:",
];

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_restored_run_reuses_what_it_computed_for_a_crate_that_uses_a_derive() -> Result<()> {
    cordial::init_tracing();
    let metrics = support::semantic_metrics::QueryMetrics::default();
    let subscriber = tracing_subscriber::registry()
        .with(metrics.clone())
        .with(cordial::tracing_fmt_layer());
    let _subscriber = tracing::subscriber::set_default(subscriber);
    let root = specimen()?;
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = SavedState::at(state.path().join("analysis-state"));

    assert!(!run(root.path(), &saved)?);
    let executed = |family: &str| -> usize {
        metrics
            .snapshot()
            .iter()
            .filter(|(key, _)| key.starts_with(family))
            .map(|(_, count)| *count)
            .sum()
    };
    let cold: Vec<usize> = PER_FILE.iter().map(|family| executed(family)).collect();
    assert!(
        cold.iter().all(|count| *count > 0),
        "every family runs in a cold run: {cold:?}"
    );

    assert!(
        run(root.path(), &saved)?,
        "the second run starts from the first's state"
    );
    for (family, before) in PER_FILE.iter().zip(&cold) {
        assert_eq!(
            executed(family),
            *before,
            "{family} ran again in the restored run"
        );
    }
    Ok(())
}

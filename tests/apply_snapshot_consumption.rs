//! Apply edits only snapshot-checked text, and only once per file.
#![cfg(all(
    feature = "tracing",
    feature = "crate_attrs",
    feature = "impl_coverage"
))]

use miette::{IntoDiagnostic, Result};
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use tracing_subscriber::{Layer, layer::Context, prelude::*};

#[derive(Clone, Default)]
struct SourceAccess {
    edits: Arc<AtomicUsize>,
    mutate: Option<PathBuf>,
    mutation_failed: Arc<AtomicBool>,
}

impl<S: tracing::Subscriber> Layer<S> for SourceAccess {
    fn on_new_span(
        &self,
        attributes: &tracing::span::Attributes<'_>,
        _id: &tracing::Id,
        _context: Context<'_, S>,
    ) {
        let metadata = attributes.metadata();
        if metadata.target() == "cordial::loader::source::edit" && metadata.name() == "edit_file" {
            self.edits.fetch_add(1, Ordering::Relaxed);
            if let Some(path) = &self.mutate {
                let failed =
                    std::fs::write(path, "// concurrent edit\npub fn pending() {}\n").is_err();
                self.mutation_failed.store(failed, Ordering::Relaxed);
            }
        }
    }
}

fn fixture() -> Result<tempfile::TempDir> {
    let fixture = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir(fixture.path().join("src")).into_diagnostic()?;
    std::fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname='snapshot_apply'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    std::fs::write(fixture.path().join("src/lib.rs"), "pub fn pending() {}\n").into_diagnostic()?;
    std::fs::write(
        fixture.path().join("checklist.md"),
        "# Tracing instrument checklist\n\n## `snapshot_apply`\n\n- [ ] `pending` — `src/lib.rs:1` (pub)\n",
    ).into_diagnostic()?;
    Ok(fixture)
}

#[test]
fn tracing_apply_uses_graph_identity_and_text() -> Result<()> {
    cordial::init_tracing();
    let fixture = fixture()?;
    let access = SourceAccess::default();
    let subscriber = tracing_subscriber::registry()
        .with(access.clone())
        .with(cordial::tracing_fmt_layer());
    let _subscriber = tracing::subscriber::set_default(subscriber);
    let summary = cordial::run_tracing_instrument_apply(
        fixture.path(),
        &fixture.path().join("checklist.md"),
        None,
        false,
    )
    .into_diagnostic()?;
    assert_eq!(summary.changed_functions(), 1);
    assert_eq!(access.edits.load(Ordering::Relaxed), 1);
    let source = std::fs::read_to_string(fixture.path().join("src/lib.rs")).into_diagnostic()?;
    assert!(source.contains("instrument("), "{source}");
    Ok(())
}

#[test]
fn tracing_apply_rejects_a_disk_change_after_collection() -> Result<()> {
    cordial::init_tracing();
    let fixture = fixture()?;
    let path = fixture.path().join("src/lib.rs");
    let access = SourceAccess {
        mutate: Some(path.clone()),
        ..SourceAccess::default()
    };
    let subscriber = tracing_subscriber::registry()
        .with(access.clone())
        .with(cordial::tracing_fmt_layer());
    let _subscriber = tracing::subscriber::set_default(subscriber);
    let result = cordial::run_tracing_instrument_apply(
        fixture.path(),
        &fixture.path().join("checklist.md"),
        None,
        false,
    );
    assert!(!access.mutation_failed.load(Ordering::Relaxed));
    assert!(result.is_err());
    assert_eq!(access.edits.load(Ordering::Relaxed), 1);
    assert_eq!(
        std::fs::read_to_string(path).into_diagnostic()?,
        "// concurrent edit\npub fn pending() {}\n"
    );
    Ok(())
}

#[test]
fn crate_attribute_apply_uses_graph_identity_and_text() -> Result<()> {
    cordial::init_tracing();
    let fixture = fixture()?;
    let access = SourceAccess::default();
    let subscriber = tracing_subscriber::registry()
        .with(access.clone())
        .with(cordial::tracing_fmt_layer());
    let _subscriber = tracing::subscriber::set_default(subscriber);
    let summary = cordial::run_crate_attrs_apply(fixture.path(), fixture.path(), None, false)
        .into_diagnostic()?;
    assert_eq!(summary.changed_files(), 1);
    assert_eq!(summary.inserted_attrs(), 2);
    assert_eq!(access.edits.load(Ordering::Relaxed), 1);
    Ok(())
}

#[test]
fn proof_collection_reads_database_text() -> Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic()?;
    let path = fixture.path().join("proof.rs");
    std::fs::write(&path,
        "fn proof() { assert_proofs_non_empty::<Widget>(); assert_kani_contains::<Widget, bool>(); proof_chain(\"Widget\"); }\n",
    ).into_diagnostic()?;
    let access = SourceAccess::default();
    let subscriber = tracing_subscriber::registry()
        .with(access.clone())
        .with(cordial::tracing_fmt_layer());
    let _subscriber = tracing::subscriber::set_default(subscriber);
    let proof = cordial::testing::collect_proof_harness(&path).into_diagnostic()?;
    assert!(proof.non_empty_types().contains("Widget"));
    assert_eq!(
        proof.composition_pairs(),
        &[("Widget".to_string(), "bool".to_string())]
    );
    assert!(proof.proof_chain_subjects().contains("Widget"));
    Ok(())
}

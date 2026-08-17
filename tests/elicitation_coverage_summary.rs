//! Elicitation coverage summary metric parity with elicit_doc `summary.md`.

use miette::{IntoDiagnostic, WrapErr};
mod parity_support;

use std::fs;
use std::path::Path;

use cordial::{
    ELICITATION_COVERAGE, IMPL_COVERAGE_ETIQUETTE, NamedRunFilter, RunAll, SHADOW_ETIQUETTE,
    Session, SessionBuilder,
};

use parity_support::{run_cordial_impl_coverage, seed_minimal_shadow_fixture, workspace_path};

#[test]
fn elicitation_summary_impl_table_has_elicit_doc_columns() -> miette::Result<()> {
    let workspace = workspace_path("minimal-workspace");
    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    run_cordial_impl_coverage(&workspace, store.path(), Some("url"))?;

    let session = SessionBuilder::new(&workspace)
        .with_store_root(store.path())
        .register_plugin(&ELICITATION_COVERAGE)
        .build();
    let filter = NamedRunFilter::etiquettes(&["impl-coverage"]).with_crate("url".to_string());
    session.run(&filter).into_diagnostic().wrap_err("run")?;

    let body = fs::read_to_string(store.path().join("findings/summary.md"))
        .into_diagnostic()
        .wrap_err("summary")?;
    assert!(body.contains("OurTraitsDone"));
    assert!(body.contains("ElicitCompleteGap"));
    assert!(body.contains("ExternallyBlocked"));
    assert!(body.contains("## Impl Coverage"));
    Ok(())
}

#[test]
fn elicitation_summary_includes_shadow_section_for_minimal_workspace() -> miette::Result<()> {
    let workspace = workspace_path("minimal-workspace");
    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    seed_minimal_shadow_fixture(&workspace, store.path())?;

    let session = SessionBuilder::new(&workspace)
        .with_store_root(store.path())
        .register(&SHADOW_ETIQUETTE)
        .register(&IMPL_COVERAGE_ETIQUETTE)
        .build();
    let filter =
        NamedRunFilter::etiquettes(&["shadow", "impl-coverage"]).with_crate("url".to_string());
    session.run(&filter).into_diagnostic().wrap_err("run")?;

    let body = fs::read_to_string(store.path().join("findings/summary.md"))
        .into_diagnostic()
        .wrap_err("summary")?;
    assert!(body.contains("## Shadow Coverage"));
    assert!(body.contains("VerificationGaps"));
    assert!(body.contains("elicit_url"));
    assert!(body.contains("## Target Support (core + shadow)"));
    assert!(body.contains("CoreTracked"));
    Ok(())
}

#[test]
fn full_elicitation_run_writes_summary_md() -> miette::Result<()> {
    let workspace =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/parity/workspaces/minimal-workspace");
    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    seed_minimal_shadow_fixture(&workspace, store.path())?;

    let session = SessionBuilder::new(&workspace)
        .with_store_root(store.path())
        .register_plugin(&ELICITATION_COVERAGE)
        .build();
    session.run(&RunAll).into_diagnostic().wrap_err("run")?;

    assert!(store.path().join("findings/summary.md").is_file());
    assert!(
        store
            .path()
            .join("findings/shadow-core-support.json")
            .is_file()
    );
    Ok(())
}

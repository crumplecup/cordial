//! Proof harness scanning for elicitation impl coverage.

use miette::{IntoDiagnostic, WrapErr};
use std::fs;
use std::path::Path;

use cordial::testing::{
    ProofHarness, TestStatus, collect_proof_harness, test_status_for_type_path,
};

#[test]
fn collect_proof_harness_parses_non_empty_and_kani_contains() -> miette::Result<()> {
    cordial::init_tracing();
    let dir = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let path = dir.path().join("harness.rs");
    fs::write(
        &path,
        r#"
            assert_proofs_non_empty::<Widget>();
            assert_kani_contains::<Widget, bool>();
        "#,
    )
    .into_diagnostic()
    .wrap_err("write harness")?;

    let harness = collect_proof_harness(&path)
        .into_diagnostic()
        .wrap_err("scan")?;
    assert!(harness.non_empty_types.contains("Widget"));
    assert_eq!(
        harness.composition_pairs,
        vec![("Widget".to_string(), "bool".to_string())]
    );
    Ok(())
}

#[test]
fn collect_proof_harness_parses_qualified_non_empty_types() -> miette::Result<()> {
    cordial::init_tracing();
    let dir = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let path = dir.path().join("harness.rs");
    fs::write(&path, "assert_proofs_non_empty::<url::Widget>();\n")
        .into_diagnostic()
        .wrap_err("write harness")?;

    let harness = collect_proof_harness(&path)
        .into_diagnostic()
        .wrap_err("scan")?;
    let (proof, composition) = test_status_for_type_path("url::Widget", false, &harness);
    assert_eq!(proof.display(), "Covered");
    assert_eq!(composition.display(), "Missing");
    Ok(())
}

#[test]
fn minimal_workspace_fixture_links_widget_to_proof_harness() -> miette::Result<()> {
    cordial::init_tracing();
    let workspace =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/parity/workspaces/minimal-workspace");
    let harness_path = workspace.join("crates/elicitation/tests/proof_non_empty_test.rs");
    assert!(harness_path.is_file(), "missing {}", harness_path.display());

    let harness = collect_proof_harness(&harness_path)
        .into_diagnostic()
        .wrap_err("scan fixture harness")?;
    let (proof, composition) = test_status_for_type_path("url::Widget", false, &harness);
    assert_eq!(proof.display(), "Covered");
    assert_eq!(composition.display(), "Missing");
    Ok(())
}

#[test]
fn test_status_handles_nested_generics_in_turbofish() {
    cordial::init_tracing();
    let harness = ProofHarness {
        non_empty_types: ["HashMap<String, Vec<bool>>".to_string()]
            .into_iter()
            .collect(),
        ..ProofHarness::default()
    };
    let (proof, _) = test_status_for_type_path("HashMap", true, &harness);
    assert!(matches!(proof, TestStatus::CoveredConcrete { .. }));
}

//! Native signature evidence preserves contracts through virtual revisions.
#![cfg(feature = "internal_error_chain")]

#[path = "support/error_sites_default.rs"]
mod default_sites;
#[path = "support/error_sites_projection.rs"]
mod projection;

use cordial::{CrateTarget, IrView};
use miette::{IntoDiagnostic, Result};

const HOOKS: &[&str] = &["error-signature-inventory"];
const ATTR: &str = "internal_error_record_kind";

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn canonical_signatures_match_complete_facts_and_virtual_contracts() -> Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic()?;
    let root = fixture.path();
    std::fs::create_dir_all(root.join("src")).into_diagnostic()?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname=\"signature_parity\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[workspace]\n",
    )
    .into_diagnostic()?;
    let library = root.join("src/lib.rs");
    let source = format!(
        "mod child;\n{}",
        include_str!("fixtures/signature_chain_fixture.rs")
    );
    std::fs::write(&library, &source).into_diagnostic()?;
    std::fs::write(
        root.join("src/child.rs"),
        include_str!("fixtures/semantic_backend/error_signatures.rs.txt"),
    )
    .into_diagnostic()?;
    std::fs::write(root.join("src/empty.rs"), "// no declarations\n").into_diagnostic()?;
    let target = CrateTarget::new("signature_parity", root);
    let baseline = projection::baseline(root, &target)?;
    let expected = baseline.source_error_signatures().into_diagnostic()?;
    let mut database = cordial::SourceDatabase::load(root).into_diagnostic()?;
    let native = database.project(&target, &[]).into_diagnostic()?;
    let actual = native.source_error_signatures().into_diagnostic()?;
    // Compare each complete group to make a mismatch readable without hiding payload changes.
    let expected_value = serde_json::to_value(&expected).into_diagnostic()?;
    let actual_value = serde_json::to_value(&actual).into_diagnostic()?;
    for group in ["functions", "errors", "manual_impls", "imports"] {
        assert_eq!(
            actual_value["index"][group], expected_value["index"][group],
            "{group}"
        );
    }
    let actual_conversions = actual_value["index"]["conversions"]
        .as_array()
        .ok_or_else(|| miette::miette!("native conversions must be an array"))?;
    let expected_conversions = expected_value["index"]["conversions"]
        .as_array()
        .ok_or_else(|| miette::miette!("baseline conversions must be an array"))?;
    assert_eq!(actual_conversions.len(), expected_conversions.len());
    for (index, (actual, expected)) in actual_conversions
        .iter()
        .zip(expected_conversions)
        .enumerate()
    {
        assert_eq!(actual, expected, "conversion {index}");
    }
    assert_eq!(actual, expected, "complete records and snapshots");
    assert_eq!(actual.files().len(), 3);
    let assessment = actual.assess("signature_parity", false).into_diagnostic()?;
    assert!(assessment.checks().len() > 20);
    assert_eq!(
        assessment,
        expected
            .assess("signature_parity", false)
            .into_diagnostic()?
    );
    let rows = projection::materialized_with(&native, root, HOOKS, ATTR)?;
    assert_eq!(
        rows,
        projection::materialized_with(&baseline, root, HOOKS, ATTR)?
    );
    let changed_source = source.replace("Some(&self.source)", "None");
    database
        .change_file(&library, changed_source)
        .into_diagnostic()?;
    let changed = database
        .project(&target, &[])
        .into_diagnostic()?
        .source_error_signatures()
        .into_diagnostic()?;
    assert_ne!(actual, changed);
    assert_ne!(
        assessment,
        changed
            .assess("signature_parity", false)
            .into_diagnostic()?,
        "a source contract edit must invalidate callers"
    );
    assert_eq!(std::fs::read_to_string(&library).into_diagnostic()?, source);
    database.change_file(&library, source).into_diagnostic()?;
    assert_eq!(
        database
            .project(&target, &[])
            .into_diagnostic()?
            .source_error_signatures()
            .into_diagnostic()?,
        actual
    );
    database
        .change_file(&root.join("src/empty.rs"), "fn invalid( {".into())
        .into_diagnostic()?;
    assert!(database.project(&target, &[]).is_err());
    drop(database);
    std::fs::remove_dir_all(root.join("src")).into_diagnostic()?;
    assert!(!default_sites::materialized(&native, root)?.is_empty());
    assert_eq!(
        actual.assess("signature_parity", false).into_diagnostic()?,
        assessment
    );
    assert_eq!(
        projection::materialized_with(&native, root, HOOKS, ATTR)?,
        rows
    );
    Ok(())
}

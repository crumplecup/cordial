//! Signature assessments replay from complete loader evidence without source.
#![cfg(feature = "internal_error_chain")]

#[path = "support/error_sites_default.rs"]
mod default_sites;
#[path = "support/error_sites_projection.rs"]
mod projection;

use cordial::{ATTR_SOURCE_ERROR_SIGNATURES, CrateIr, CrateTarget, IrView, SignatureChainStatus};
use miette::{IntoDiagnostic, Result};

const HOOKS: &[&str] = &["error-signature-inventory"];
const ATTR: &str = "internal_error_record_kind";

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn signatures_are_required_complete_and_replay_without_source() -> Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic()?;
    let root = fixture.path();
    std::fs::create_dir_all(root.join("src")).into_diagnostic()?;
    std::fs::write(
        root.join("src/lib.rs"),
        include_str!("fixtures/signature_chain_fixture.rs"),
    )
    .into_diagnostic()?;
    std::fs::write(root.join("src/empty.rs"), "// no declarations\n").into_diagnostic()?;
    let target = CrateTarget::new("signature_replay", root);
    let graph = projection::baseline(root, &target)?;
    let inventory = graph.source_error_signatures().into_diagnostic()?;
    assert_eq!(inventory.files().len(), 2);
    let assessment = inventory
        .assess("signature_replay", false)
        .into_diagnostic()?;
    let standalone =
        cordial::scan_crate_internal_error_chain(root, "signature_replay").into_diagnostic()?;
    assert_eq!(assessment.checks(), standalone.signature_checks());
    assert_eq!(assessment.findings().len(), 4);
    for finding in assessment.findings() {
        assert!(standalone.compliance().findings().contains(finding));
    }
    for (status, count) in [
        (SignatureChainStatus::Broken, 4),
        (SignatureChainStatus::Preserved, 3),
        (SignatureChainStatus::Unknown, 1),
    ] {
        assert_eq!(
            assessment
                .checks()
                .iter()
                .filter(|check| check.status() == status)
                .count(),
            count
        );
    }
    let encoded = serde_json::to_value(&inventory).into_diagnostic()?;
    let decoded: cordial::SourceErrorSignatures =
        serde_json::from_value(encoded.clone()).into_diagnostic()?;
    assert_eq!(decoded, inventory, "complete retained facts");
    assert_eq!(
        decoded
            .assess("signature_replay", false)
            .into_diagnostic()?,
        assessment
    );
    for key in ["crate_root", "files", "index"] {
        let mut broken = encoded.clone();
        broken
            .as_object_mut()
            .ok_or_else(|| miette::miette!("inventory"))?
            .remove(key);
        let mut corrupt = graph.clone();
        corrupt
            .set_attr(corrupt.root(), ATTR_SOURCE_ERROR_SIGNATURES, broken)
            .into_diagnostic()?;
        assert!(corrupt.source_error_signatures().is_err(), "required {key}");
    }
    for key in [
        "functions",
        "errors",
        "manual_impls",
        "imports",
        "conversions",
    ] {
        let mut broken = encoded.clone();
        broken["index"]
            .as_object_mut()
            .ok_or_else(|| miette::miette!("index"))?
            .remove(key);
        let mut corrupt = graph.clone();
        corrupt
            .set_attr(corrupt.root(), ATTR_SOURCE_ERROR_SIGNATURES, broken)
            .into_diagnostic()?;
        assert!(corrupt.source_error_signatures().is_err(), "required {key}");
    }
    let mut changed = encoded;
    changed["index"]["manual_impls"]["ChainError"] = serde_json::json!(["Default"]);
    let changed: cordial::SourceErrorSignatures =
        serde_json::from_value(changed).into_diagnostic()?;
    assert_ne!(
        changed
            .assess("signature_replay", false)
            .into_diagnostic()?,
        assessment,
        "retained source contracts must affect outcomes, even with unchanged ids and counts"
    );
    assert!(CrateIr::new("empty").source_error_signatures().is_err());
    let rows = projection::materialized_with(&graph, root, HOOKS, ATTR)?;
    assert_eq!(rows.len(), 4);
    std::fs::remove_dir_all(root.join("src")).into_diagnostic()?;
    assert!(!default_sites::materialized(&graph, root)?.is_empty());
    assert_eq!(
        inventory
            .assess("signature_replay", false)
            .into_diagnostic()?,
        assessment
    );
    assert_eq!(
        projection::materialized_with(&graph, root, HOOKS, ATTR)?,
        rows
    );
    Ok(())
}

//! Compliance contracts preserve complete evidence and source-free replay.
#![cfg(feature = "internal_error_chain")]

#[path = "support/error_sites_default.rs"]
mod default_sites;
#[path = "support/error_sites_projection.rs"]
mod projection;

use cordial::{ATTR_SOURCE_ERROR_COMPLIANCE, CrateIr, CrateTarget, IrView};
use miette::{IntoDiagnostic, Result};

const HOOKS: &[&str] = &["error-compliance-inventory"];
const ATTR: &str = "internal_error_record_kind";
const CHILD: &str =
    "fn loss() { builder.build().map_err(|cause| Local::new(cause.to_string())); }\n";

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn loader_compliance_is_required_and_replays_without_source() -> Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic()?;
    let root = fixture.path();
    std::fs::create_dir_all(root.join("src")).into_diagnostic()?;
    std::fs::write(root.join("src/lib.rs"), CHILD).into_diagnostic()?;
    std::fs::write(root.join("src/empty.rs"), "// no findings\n").into_diagnostic()?;
    let target = CrateTarget::new("compliance_replay", root);
    let graph = projection::baseline(root, &target)?;
    let inventory = graph.source_error_compliance().into_diagnostic()?;
    assert_eq!(inventory.files().len(), 2);
    let findings = inventory.findings(false);
    assert_eq!(findings.len(), 1);
    let standalone =
        cordial::scan_crate_internal_error_chain(root, "compliance_replay").into_diagnostic()?;
    assert_eq!(&findings, standalone.compliance().findings());
    let rows = projection::materialized_with(&graph, root, HOOKS, ATTR)?;
    assert_eq!(rows.len(), 1);
    let value = serde_json::to_value(&inventory).into_diagnostic()?;
    let index = inventory
        .files()
        .iter()
        .position(|file| !file.findings().is_empty())
        .ok_or_else(|| miette::miette!("compliance evidence"))?;
    for (container, keys) in [
        ("file", vec!["snapshot", "findings"]),
        (
            "finding",
            vec![
                "crate_name",
                "rule_id",
                "context",
                "file",
                "line",
                "snippet",
            ],
        ),
    ] {
        for key in keys {
            let mut broken = value.clone();
            let slot = if container == "file" {
                &mut broken["files"][index]
            } else {
                &mut broken["files"][index]["findings"][0]
            };
            slot.as_object_mut()
                .ok_or_else(|| miette::miette!("inventory object"))?
                .remove(key);
            let mut corrupt = graph.clone();
            corrupt
                .set_attr(corrupt.root(), ATTR_SOURCE_ERROR_COMPLIANCE, broken)
                .into_diagnostic()?;
            assert!(corrupt.source_error_compliance().is_err(), "required {key}");
        }
    }
    assert!(CrateIr::new("empty").source_error_compliance().is_err());
    std::fs::remove_dir_all(root.join("src")).into_diagnostic()?;
    assert_eq!(inventory.findings(false), findings);
    assert!(!default_sites::materialized(&graph, root)?.is_empty());
    assert_eq!(
        projection::materialized_with(&graph, root, HOOKS, ATTR)?,
        rows
    );
    Ok(())
}

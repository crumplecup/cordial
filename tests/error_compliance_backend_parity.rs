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

// Slow (over 60s): compiled only without the default `quick` feature; run with `just test-slow`.
#[cfg(not(feature = "quick"))]
#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn canonical_compliance_payloads_follow_virtual_revisions() -> Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic()?;
    let root = fixture.path();
    std::fs::create_dir_all(root.join("src")).into_diagnostic()?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname=\"compliance_parity\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[workspace]\n",
    )
    .into_diagnostic()?;
    let library = root.join("src/lib.rs");
    let child = root.join("src/child.rs");
    let source = include_str!("fixtures/semantic_backend/error_compliance.rs.txt");
    std::fs::write(&library, source).into_diagnostic()?;
    std::fs::write(&child, CHILD).into_diagnostic()?;
    std::fs::write(root.join("src/empty.rs"), "// no findings\n").into_diagnostic()?;
    let target = CrateTarget::new("compliance_parity", root);
    let baseline = projection::baseline(root, &target)?;
    let expected = baseline.source_error_compliance().into_diagnostic()?;
    let mut database = cordial::SourceDatabase::load(root).into_diagnostic()?;
    let native = database.project(&target, &[]).into_diagnostic()?;
    let actual = native.source_error_compliance().into_diagnostic()?;
    for finding in actual.findings(false) {
        assert!(
            expected.findings(false).contains(&finding),
            "unexpected {finding:?}"
        );
    }
    for finding in expected.findings(false) {
        assert!(
            actual.findings(false).contains(&finding),
            "missing {finding:?}"
        );
    }
    assert_eq!(actual, expected, "complete records and snapshots");
    assert_eq!(actual.files().len(), 3);
    let findings = actual.findings(true);
    assert!(
        findings.len() > 30,
        "fixture must exercise each expression family"
    );
    for suffix in [
        "losses",
        "known_source",
        "dormant::loss",
        "included::loss",
        "child::loss",
    ] {
        assert!(
            findings.iter().any(|item| item.context().ends_with(suffix)),
            "missing {suffix}"
        );
    }
    assert!(
        !findings
            .iter()
            .any(|item| item.context().contains("excluded")
                || item.context().ends_with("preserving"))
    );
    let rows = projection::materialized_with(&native, root, HOOKS, ATTR)?;
    assert_eq!(
        rows,
        projection::materialized_with(&baseline, root, HOOKS, ATTR)?
    );
    let macro_target = target
        .clone()
        .with_kinds(vec![cordial::CrateKind::ProcMacro]);
    let macro_baseline = projection::baseline(root, &macro_target)?
        .source_error_compliance()
        .into_diagnostic()?;
    let macro_native = database
        .project(&macro_target, &[])
        .into_diagnostic()?
        .source_error_compliance()
        .into_diagnostic()?;
    assert_eq!(macro_native, macro_baseline, "proc-macro terminus policy");
    assert!(macro_native.findings(false).len() < findings.len());
    database
        .change_file(&child, "// resolved\n".into())
        .into_diagnostic()?;
    let changed = database
        .project(&target, &[])
        .into_diagnostic()?
        .source_error_compliance()
        .into_diagnostic()?;
    assert_ne!(changed, actual);
    assert!(
        !changed
            .findings(false)
            .iter()
            .any(|finding| finding.context() == "child::loss")
    );
    assert_eq!(std::fs::read_to_string(&child).into_diagnostic()?, CHILD);
    database
        .change_file(&child, CHILD.into())
        .into_diagnostic()?;
    assert_eq!(
        database
            .project(&target, &[])
            .into_diagnostic()?
            .source_error_compliance()
            .into_diagnostic()?,
        actual
    );
    database
        .change_file(&root.join("src/empty.rs"), "fn invalid( {".into())
        .into_diagnostic()?;
    assert!(database.project(&target, &[]).is_err());
    drop(database);
    std::fs::remove_dir_all(root.join("src")).into_diagnostic()?;
    assert_eq!(
        projection::materialized_with(&native, root, HOOKS, ATTR)?,
        rows
    );
    Ok(())
}

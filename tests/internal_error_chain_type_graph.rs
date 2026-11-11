//! The type graph: signature chains, the architecture catalog, internal leaves, foreign bridges, and `src/error.rs`.

use miette::{IntoDiagnostic, WrapErr};
use std::fs;
#[path = "support/error_reports.rs"]
mod error_reports;
use cordial::{
    InternalErrorComplianceId, InternalErrorNodeClass, InternalErrorTypeProbeId,
    SignatureChainStatus,
};

#[path = "support/error_chain_crate.rs"]
mod error_chain_crate;

use error_chain_crate::crate_with_lib;

const TYPE_GRAPH_FIXTURE: &str = include_str!("fixtures/error_chain/type_graph.rs");

/// Type-graph nodes for `source` placed as the lib of a throwaway crate.
fn type_nodes(source: &str) -> miette::Result<Vec<cordial::InternalErrorTypeNode>> {
    let fixture = crate_with_lib(source, false)?;
    Ok(error_reports::internal_report(fixture.path(), "fixture")?
        .type_graph()
        .nodes()
        .clone())
}

const ERROR_RS_FIXTURE: &str = include_str!("fixtures/error_chain/error_rs.rs");

#[test]
fn signature_chain_catches_helpers_across_modules() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic()?;
    let src = fixture.path().join("src");
    fs::create_dir_all(&src).into_diagnostic()?;
    fs::write(
        src.join("lib.rs"),
        include_str!("fixtures/signature_chain_fixture.rs"),
    )
    .into_diagnostic()?;

    let report = error_reports::internal_report(fixture.path(), "fixture")?;
    let signature_findings: Vec<_> = report
        .compliance()
        .findings()
        .iter()
        .filter(|finding| {
            finding.rule_id() == InternalErrorComplianceId::DiscardTyped001
                && finding.snippet().starts_with("map_err conversion:")
        })
        .collect();
    assert_eq!(signature_findings.len(), 4, "{signature_findings:#?}");
    assert!(
        signature_findings
            .iter()
            .any(|finding| finding.context().is_empty())
    );
    assert_eq!(
        signature_findings
            .iter()
            .filter(|finding| finding
                .snippet()
                .contains("MessageError cannot preserve SourceError"))
            .count(),
        3
    );
    assert!(signature_findings.iter().any(|finding| {
        finding
            .snippet()
            .contains("FakeChainError cannot preserve SourceError")
    }));
    assert_eq!(
        report
            .signature_checks()
            .iter()
            .filter(|check| check.status() == SignatureChainStatus::Broken)
            .count(),
        4
    );
    assert_eq!(
        report
            .signature_checks()
            .iter()
            .filter(|check| check.status() == SignatureChainStatus::Preserved)
            .count(),
        3
    );
    assert_eq!(
        report
            .signature_checks()
            .iter()
            .filter(|check| check.status() == SignatureChainStatus::Unknown)
            .count(),
        1
    );
    Ok(())
}

#[test]
fn architecture_catalog_handles_impl_before_type_definition() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic()?;
    let src = fixture.path().join("src");
    fs::create_dir_all(&src).into_diagnostic()?;
    fs::write(
        src.join("lib.rs"),
        r#"
impl TrackedError {
    #[track_caller]
    fn new(_: std::io::Error) -> Self { Self }
}

#[derive(Debug)]
struct TrackedError;
impl std::fmt::Display for TrackedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "tracked")
    }
}
impl std::error::Error for TrackedError {}

fn site() {
    let _ = std::fs::read("x").map_err(TrackedError::new);
}
"#,
    )
    .into_diagnostic()?;

    let report = error_reports::internal_report(fixture.path(), "fixture")?;
    let tracked_sites = report
        .compliance()
        .findings()
        .iter()
        .filter(|finding| finding.rule_id() == InternalErrorComplianceId::MapErrTrackCaller001)
        .count();
    assert_eq!(tracked_sites, 1);
    Ok(())
}

#[test]
fn internal_leaf_is_detected_in_type_graph() -> miette::Result<()> {
    cordial::init_tracing();
    let nodes = type_nodes(TYPE_GRAPH_FIXTURE)?;
    assert!(nodes.iter().any(|node| {
        node.probe_id() == InternalErrorTypeProbeId::InternalLeaf001
            && node.node_class() == InternalErrorNodeClass::InternalLeaf
            && node.type_path().contains("Invariant")
    }));
    Ok(())
}

#[test]
fn foreign_bridge_is_detected_in_type_graph() -> miette::Result<()> {
    cordial::init_tracing();
    let nodes = type_nodes(TYPE_GRAPH_FIXTURE)?;
    assert!(nodes.iter().any(|node| {
        node.node_class() == InternalErrorNodeClass::ForeignBridge
            && node.type_path().ends_with("InnerSource")
    }));
    Ok(())
}

#[test]
fn nested_source_impl_is_detected() -> miette::Result<()> {
    cordial::init_tracing();
    let nodes = type_nodes(TYPE_GRAPH_FIXTURE)?;
    assert!(nodes.iter().any(|node| {
        node.probe_id() == InternalErrorTypeProbeId::InternalNested001
            && node.type_path() == "DomainError"
    }));
    Ok(())
}

#[test]
fn type_graph_scans_src_error_rs() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    fs::write(fixture.path().join("src/error.rs"), ERROR_RS_FIXTURE)
        .into_diagnostic()
        .wrap_err("write error.rs")?;
    fs::write(fixture.path().join("src/lib.rs"), "mod error;\n")
        .into_diagnostic()
        .wrap_err("write lib")?;

    let report = error_reports::internal_report(fixture.path(), "fixture")?;
    assert!(
        report.type_graph().nodes().iter().any(|node| {
            node.type_path().contains("SynParse")
                && node.node_class() != InternalErrorNodeClass::InternalLeaf
        }),
        "SynParse {{ err }} should keep the foreign error: {:?}",
        report.type_graph().nodes()
    );
    assert!(
        report
            .type_graph()
            .nodes()
            .iter()
            .any(|node| node.type_path().contains("Io")),
        "Io(std::io::Error) should appear in the type graph: {:?}",
        report.type_graph().nodes()
    );
    assert!(
        report
            .type_graph()
            .nodes()
            .iter()
            .any(|node| node.type_path().contains("Invariant")
                && node.node_class() == InternalErrorNodeClass::InternalLeaf),
        "Invariant should remain a leaf: {:?}",
        report.type_graph().nodes()
    );
    assert!(
        report.type_graph().nodes().iter().any(|node| {
            node.type_path().contains("JsonParse")
                && node.node_class() != InternalErrorNodeClass::InternalLeaf
        }),
        "JsonParse {{ err }} should keep the foreign error: {:?}",
        report.type_graph().nodes()
    );
    assert!(
        report.type_graph().nodes().iter().any(|node| {
            node.type_path().contains("CargoMetadata")
                && node.node_class() != InternalErrorNodeClass::InternalLeaf
        }),
        "CargoMetadata(cargo_metadata::Error) should keep the foreign error: {:?}",
        report.type_graph().nodes()
    );
    Ok(())
}

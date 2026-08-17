use miette::{IntoDiagnostic, WrapErr};
use std::fs;
use std::path::Path;

use cordial::{
    INTERNAL_ERROR_CHAIN_ETIQUETTE, InternalErrorComplianceId, InternalErrorNodeClass,
    InternalErrorTypeProbeId, RunAll, Session, SessionBuilder, scan_compliance_rust_source,
    scan_crate_internal_error_chain, scan_error_rust_source,
};

const TYPE_GRAPH_FIXTURE: &str = r#"
use std::error::Error;

#[derive(Debug)]
enum DomainError {
    Invariant { detail: String },
    Wrapped { source: InnerSource },
}

#[derive(Debug)]
struct InnerSource {
    source: std::io::Error,
}

impl Error for DomainError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Wrapped { source } => Some(source),
            Self::Invariant { .. } => None,
        }
    }
}
"#;

const COMPLIANCE_FIXTURE: &str = r#"
use cordial::{CordialError, CordialResult};

fn stringify_foreign() -> CordialResult<()> {
    std::fs::read_to_string("x").map_err(|e| {
        std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
    })?;
    Ok(())
}

fn discard_typed() -> CordialResult<()> {
    std::fs::read_to_string("x").map_err(|e| CordialError::invariant(e.to_string()))?;
    Ok(())
}

fn if_let_discard(r: Result<i32, std::io::Error>) -> CordialResult<()> {
    if let Err(e) = r {
        return Err(CordialError::invariant(e.to_string()));
    }
    Ok(())
}

fn preserved() -> CordialResult<()> {
    std::fs::read_to_string("x")?;
    Ok(())
}

fn wrap_syn() -> CordialResult<()> {
    syn::parse_file("").map_err(|err| CordialError::syn_parse("x.rs", err))?;
    Ok(())
}

fn wrap_syn_display_path(path: &std::path::Path) -> CordialResult<()> {
    syn::parse_file("").map_err(|err| CordialError::syn_parse(path.display().to_string(), err))?;
    Ok(())
}

fn wrap_from() -> CordialResult<()> {
    std::fs::read_to_string("x").map_err(CordialError::from)?;
    Ok(())
}

fn wrap_json(path: &std::path::Path) -> CordialResult<()> {
    serde_json::from_str::<i32>("x")
        .map_err(|err| CordialError::json_parse(path.display().to_string(), err))?;
    Ok(())
}

fn wrap_cargo_metadata() -> CordialResult<()> {
    cargo_metadata::MetadataCommand::new()
        .exec()
        .map_err(CordialError::cargo_metadata)?;
    Ok(())
}

fn stringify_via_format() -> CordialResult<()> {
    serde_json::from_str::<i32>("x").map_err(|err| {
        CordialError::invariant(format!("parse failed: {err}"))
    })?;
    Ok(())
}
"#;

#[test]
fn internal_leaf_is_detected_in_type_graph() -> miette::Result<()> {
    let error_root = Path::new("tests/fixtures/internal_error_chain");
    let file = error_root.join("type_graph.rs");
    fs::create_dir_all(error_root)
        .into_diagnostic()
        .wrap_err("fixture dir")?;
    fs::write(&file, TYPE_GRAPH_FIXTURE)
        .into_diagnostic()
        .wrap_err("write fixture")?;
    let nodes = scan_error_rust_source(TYPE_GRAPH_FIXTURE, &file, error_root, "fixture")
        .into_diagnostic()
        .wrap_err("scan type graph")?;
    assert!(nodes.iter().any(|node| {
        node.probe_id == InternalErrorTypeProbeId::InternalLeaf001
            && node.node_class == InternalErrorNodeClass::InternalLeaf
            && node.type_path.contains("Invariant")
    }));
    Ok(())
}

#[test]
fn foreign_bridge_is_detected_in_type_graph() -> miette::Result<()> {
    let error_root = Path::new("tests/fixtures/internal_error_chain");
    let file = error_root.join("type_graph.rs");
    fs::create_dir_all(error_root)
        .into_diagnostic()
        .wrap_err("fixture dir")?;
    fs::write(&file, TYPE_GRAPH_FIXTURE)
        .into_diagnostic()
        .wrap_err("write fixture")?;
    let nodes = scan_error_rust_source(TYPE_GRAPH_FIXTURE, &file, error_root, "fixture")
        .into_diagnostic()
        .wrap_err("scan type graph")?;
    assert!(nodes.iter().any(|node| {
        node.node_class == InternalErrorNodeClass::ForeignBridge
            && node.type_path.ends_with("InnerSource")
    }));
    Ok(())
}

#[test]
fn nested_source_impl_is_detected() -> miette::Result<()> {
    let error_root = Path::new("tests/fixtures/internal_error_chain");
    let file = error_root.join("type_graph.rs");
    fs::create_dir_all(error_root)
        .into_diagnostic()
        .wrap_err("fixture dir")?;
    fs::write(&file, TYPE_GRAPH_FIXTURE)
        .into_diagnostic()
        .wrap_err("write fixture")?;
    let nodes = scan_error_rust_source(TYPE_GRAPH_FIXTURE, &file, error_root, "fixture")
        .into_diagnostic()
        .wrap_err("scan type graph")?;
    assert!(nodes.iter().any(|node| {
        node.probe_id == InternalErrorTypeProbeId::InternalNested001
            && node.type_path == "DomainError"
    }));
    Ok(())
}

#[test]
fn stringify_map_err_is_compliance_violation() -> miette::Result<()> {
    let src_root = Path::new("tests/fixtures/internal_error_chain");
    let file = src_root.join("compliance.rs");
    fs::create_dir_all(src_root)
        .into_diagnostic()
        .wrap_err("fixture dir")?;
    fs::write(&file, COMPLIANCE_FIXTURE)
        .into_diagnostic()
        .wrap_err("write fixture")?;
    let findings = scan_compliance_rust_source(COMPLIANCE_FIXTURE, &file, src_root, "fixture")
        .into_diagnostic()
        .wrap_err("scan")?;
    assert!(findings.iter().any(|finding| {
        finding.rule_id == InternalErrorComplianceId::StringifyForeign001
            && finding.context.contains("stringify_foreign")
    }));
    Ok(())
}

#[test]
fn discard_typed_error_is_compliance_violation() -> miette::Result<()> {
    let src_root = Path::new("tests/fixtures/internal_error_chain");
    let file = src_root.join("compliance.rs");
    fs::create_dir_all(src_root)
        .into_diagnostic()
        .wrap_err("fixture dir")?;
    fs::write(&file, COMPLIANCE_FIXTURE)
        .into_diagnostic()
        .wrap_err("write fixture")?;
    let findings = scan_compliance_rust_source(COMPLIANCE_FIXTURE, &file, src_root, "fixture")
        .into_diagnostic()
        .wrap_err("scan")?;
    assert!(findings.iter().any(|finding| {
        finding.rule_id == InternalErrorComplianceId::DiscardTyped001
            && finding.context.contains("discard_typed")
    }));
    assert!(findings.iter().any(|finding| {
        finding.rule_id == InternalErrorComplianceId::DiscardTyped001
            && finding.context.contains("if_let_discard")
    }));
    Ok(())
}

#[test]
fn preserved_foreign_propagation_is_not_compliance_violation() -> miette::Result<()> {
    let src_root = Path::new("tests/fixtures/internal_error_chain");
    let file = src_root.join("compliance.rs");
    fs::create_dir_all(src_root)
        .into_diagnostic()
        .wrap_err("fixture dir")?;
    fs::write(&file, COMPLIANCE_FIXTURE)
        .into_diagnostic()
        .wrap_err("write fixture")?;
    let findings = scan_compliance_rust_source(COMPLIANCE_FIXTURE, &file, src_root, "fixture")
        .into_diagnostic()
        .wrap_err("scan")?;
    assert!(
        !findings
            .iter()
            .any(|finding| finding.context.contains("preserved"))
    );
    Ok(())
}

#[test]
fn syn_parse_and_from_wrappers_are_not_compliance_violations() -> miette::Result<()> {
    let src_root = Path::new("tests/fixtures/internal_error_chain");
    let file = src_root.join("compliance.rs");
    fs::create_dir_all(src_root)
        .into_diagnostic()
        .wrap_err("fixture dir")?;
    fs::write(&file, COMPLIANCE_FIXTURE)
        .into_diagnostic()
        .wrap_err("write fixture")?;
    let findings = scan_compliance_rust_source(COMPLIANCE_FIXTURE, &file, src_root, "fixture")
        .into_diagnostic()
        .wrap_err("scan")?;
    assert!(
        !findings
            .iter()
            .any(|finding| finding.context.contains("wrap_syn")
                || finding.context.contains("wrap_from")
                || finding.context.contains("wrap_json")
                || finding.context.contains("wrap_cargo_metadata")),
        "typed wrappers that keep the foreign error must not be discards: {findings:?}"
    );
    Ok(())
}

#[test]
fn format_interpolation_of_error_binding_is_compliance_violation() -> miette::Result<()> {
    let src_root = Path::new("tests/fixtures/internal_error_chain");
    let file = src_root.join("compliance.rs");
    fs::create_dir_all(src_root)
        .into_diagnostic()
        .wrap_err("fixture dir")?;
    fs::write(&file, COMPLIANCE_FIXTURE)
        .into_diagnostic()
        .wrap_err("write fixture")?;
    let findings = scan_compliance_rust_source(COMPLIANCE_FIXTURE, &file, src_root, "fixture")
        .into_diagnostic()
        .wrap_err("scan")?;
    assert!(
        findings.iter().any(|finding| {
            finding.context.contains("stringify_via_format")
                && matches!(
                    finding.rule_id,
                    InternalErrorComplianceId::DiscardTyped001
                        | InternalErrorComplianceId::StringifyForeign001
                )
        }),
        "invariant(format!(…{{err}})) must be a compliance hit: {findings:?}"
    );
    Ok(())
}

const ERROR_RS_FIXTURE: &str = r#"
use std::error::Error;

#[derive(Debug)]
enum CordialError {
    Io(std::io::Error),
    SynParse { path: String, err: syn::Error },
    JsonParse { path: String, err: serde_json::Error },
    Invariant { message: String },
    CargoMetadata(cargo_metadata::Error),
}

impl Error for CordialError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::SynParse { err, .. } => Some(err),
            Self::JsonParse { err, .. } => Some(err),
            Self::CargoMetadata(err) => Some(err),
            Self::Invariant { .. } => None,
        }
    }
}
"#;

#[test]
fn type_graph_scans_src_error_rs() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(fixture.path().join("src/error.rs"), ERROR_RS_FIXTURE)
        .into_diagnostic()
        .wrap_err("write error.rs")?;
    fs::write(fixture.path().join("src/lib.rs"), "mod error;\n")
        .into_diagnostic()
        .wrap_err("write lib")?;

    let report = scan_crate_internal_error_chain(fixture.path(), "fixture")
        .into_diagnostic()
        .wrap_err("scan")?;
    assert!(
        report.type_graph.nodes.iter().any(|node| {
            node.type_path.contains("SynParse")
                && node.node_class != InternalErrorNodeClass::InternalLeaf
        }),
        "SynParse {{ err }} should keep the foreign error: {:?}",
        report.type_graph.nodes
    );
    assert!(
        report
            .type_graph
            .nodes
            .iter()
            .any(|node| node.type_path.contains("Io")),
        "Io(std::io::Error) should appear in the type graph: {:?}",
        report.type_graph.nodes
    );
    assert!(
        report
            .type_graph
            .nodes
            .iter()
            .any(|node| node.type_path.contains("Invariant")
                && node.node_class == InternalErrorNodeClass::InternalLeaf),
        "Invariant should remain a leaf: {:?}",
        report.type_graph.nodes
    );
    assert!(
        report.type_graph.nodes.iter().any(|node| {
            node.type_path.contains("JsonParse")
                && node.node_class != InternalErrorNodeClass::InternalLeaf
        }),
        "JsonParse {{ err }} should keep the foreign error: {:?}",
        report.type_graph.nodes
    );
    assert!(
        report.type_graph.nodes.iter().any(|node| {
            node.type_path.contains("CargoMetadata")
                && node.node_class != InternalErrorNodeClass::InternalLeaf
        }),
        "CargoMetadata(cargo_metadata::Error) should keep the foreign error: {:?}",
        report.type_graph.nodes
    );
    Ok(())
}

#[test]
fn session_produces_all_four_artifacts() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src/error"))
        .into_diagnostic()
        .wrap_err("error dir")?;
    fs::write(
        fixture.path().join("src/error/type_graph.rs"),
        TYPE_GRAPH_FIXTURE,
    )
    .into_diagnostic()
    .wrap_err("write type graph")?;
    fs::write(fixture.path().join("src/compliance.rs"), COMPLIANCE_FIXTURE)
        .into_diagnostic()
        .wrap_err("write compliance")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        "mod compliance;\npub mod error { pub mod type_graph; }",
    )
    .into_diagnostic()
    .wrap_err("write lib")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&INTERNAL_ERROR_CHAIN_ETIQUETTE)
        .build();

    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    assert!(outcome.findings().count() >= 5);

    let findings_dir = store.path().join("findings");
    let graph = fs::read_to_string(findings_dir.join("internal-error-type-graph.csv"))
        .into_diagnostic()
        .wrap_err("type graph csv")?;
    assert!(graph.contains("ERROR-CHAIN-INTERNAL-LEAF"));
    assert!(graph.contains("ERROR-CHAIN-FOREIGN-BRIDGE"));

    let compliance = fs::read_to_string(findings_dir.join("internal-error-compliance.csv"))
        .into_diagnostic()
        .wrap_err("compliance csv")?;
    assert!(compliance.contains("ERROR-CHAIN-COMPLIANCE-STRINGIFY-001"));
    assert!(compliance.contains("ERROR-CHAIN-COMPLIANCE-DISCARD-TYPED-001"));

    let checklist = fs::read_to_string(findings_dir.join("internal-error-chain.checklist.md"))
        .into_diagnostic()
        .wrap_err("checklist")?;
    assert!(checklist.contains("Internal error chain checklist"));
    assert!(checklist.contains("Compliance violations"));

    let summary = fs::read_to_string(findings_dir.join("internal-error-chain-summary.md"))
        .into_diagnostic()
        .wrap_err("summary")?;
    assert!(summary.contains("Internal error chain summary"));
    assert!(summary.contains("compliance violations"));
    Ok(())
}

#[test]
fn scan_crate_internal_error_chain_combines_both_scans() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src/error"))
        .into_diagnostic()
        .wrap_err("error dir")?;
    fs::write(
        fixture.path().join("src/error/type_graph.rs"),
        TYPE_GRAPH_FIXTURE,
    )
    .into_diagnostic()
    .wrap_err("write type graph")?;
    fs::write(fixture.path().join("src/compliance.rs"), COMPLIANCE_FIXTURE)
        .into_diagnostic()
        .wrap_err("write compliance")?;

    let report = scan_crate_internal_error_chain(fixture.path(), "fixture")
        .into_diagnostic()
        .wrap_err("combined scan")?;
    assert!(!report.type_graph.nodes.is_empty());
    assert!(!report.compliance.findings.is_empty());
    Ok(())
}

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

impl Error for InnerSource {}
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

const WELL_FORMED_SOURCE: &str = r#"
use std::io;
use std::panic::Location;

pub struct Error {
    kind: Box<ErrorKind>,
}

pub enum ErrorKind {
    Io(IoSource),
}

pub struct IoSource {
    source: io::Error,
    file: &'static str,
    line: u32,
}

impl IoSource {
    #[track_caller]
    pub fn new(source: io::Error) -> Self {
        let loc = Location::caller();
        Self {
            source,
            file: loc.file(),
            line: loc.line(),
        }
    }
}

impl From<io::Error> for IoSource {
    #[track_caller]
    fn from(source: io::Error) -> Self {
        Self::new(source)
    }
}

impl std::error::Error for Error {}
impl std::error::Error for IoSource {}
"#;

const LOCATION_FIELD_SOURCE: &str = r#"
use std::io;
use std::panic::Location;

pub struct Error {
    kind: Box<ErrorKind>,
}

pub enum ErrorKind {
    Io(IoSource),
}

pub struct IoSource {
    source: io::Error,
    location: &'static Location<'static>,
}

impl IoSource {
    #[track_caller]
    pub fn new(source: io::Error) -> Self {
        Self {
            source,
            location: Location::caller(),
        }
    }
}

impl std::error::Error for Error {}
impl std::error::Error for IoSource {}
"#;

const MISSING_TRACK_CALLER_SOURCE: &str = r#"
use std::io;

pub struct Error {
    kind: Box<ErrorKind>,
}

pub enum ErrorKind {
    Io(IoSource),
}

pub struct IoSource {
    source: io::Error,
    file: &'static str,
    line: u32,
}

impl IoSource {
    pub fn new(source: io::Error, file: &'static str, line: u32) -> Self {
        Self { source, file, line }
    }
}

impl std::error::Error for Error {}
impl std::error::Error for IoSource {}
"#;

fn write_error_crate(src: &str) -> miette::Result<tempfile::TempDir> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(fixture.path().join("src/error.rs"), src)
        .into_diagnostic()
        .wrap_err("write error.rs")?;
    fs::write(fixture.path().join("src/lib.rs"), "mod error;\n")
        .into_diagnostic()
        .wrap_err("write lib")?;
    Ok(fixture)
}

#[test]
fn incomplete_source_wrapper_is_shape_and_track_caller_violation() -> miette::Result<()> {
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

    let report = scan_crate_internal_error_chain(fixture.path(), "fixture")
        .into_diagnostic()
        .wrap_err("scan")?;
    assert!(
        report.compliance.findings.iter().any(|finding| {
            finding.rule_id == InternalErrorComplianceId::SourceShape001
                && finding.context.contains("InnerSource")
        }),
        "InnerSource without file/line must be SOURCE-SHAPE: {:?}",
        report.compliance.findings
    );
    assert!(
        report.compliance.findings.iter().any(|finding| {
            finding.rule_id == InternalErrorComplianceId::SourceTrackCaller001
                && finding.context.contains("InnerSource")
        }),
        "InnerSource without #[track_caller] constructor must be TRACK-CALLER: {:?}",
        report.compliance.findings
    );
    Ok(())
}

#[test]
fn well_formed_source_wrapper_is_not_a_compliance_violation() -> miette::Result<()> {
    let fixture = write_error_crate(WELL_FORMED_SOURCE)?;
    let report = scan_crate_internal_error_chain(fixture.path(), "fixture")
        .into_diagnostic()
        .wrap_err("scan")?;
    let source_findings: Vec<_> = report
        .compliance
        .findings
        .iter()
        .filter(|finding| {
            matches!(
                finding.rule_id,
                InternalErrorComplianceId::SourceShape001
                    | InternalErrorComplianceId::SourceTrackCaller001
            )
        })
        .collect();
    assert!(
        source_findings.is_empty(),
        "well-formed IoSource must not be flagged: {source_findings:?}"
    );
    assert!(
        !report.compliance.findings.iter().any(|finding| {
            matches!(
                finding.rule_id,
                InternalErrorComplianceId::ArchParent001
                    | InternalErrorComplianceId::ArchKindBox001
                    | InternalErrorComplianceId::ArchKindVariant001
                    | InternalErrorComplianceId::ArchOrphanSource001
            )
        }),
        "well-formed parent/Kind/source must not be architecture-flagged: {:?}",
        report.compliance.findings
    );
    Ok(())
}

#[test]
fn location_field_is_accepted_instead_of_file_and_line() -> miette::Result<()> {
    let fixture = write_error_crate(LOCATION_FIELD_SOURCE)?;
    let report = scan_crate_internal_error_chain(fixture.path(), "fixture")
        .into_diagnostic()
        .wrap_err("scan")?;
    assert!(
        !report.compliance.findings.iter().any(|finding| {
            matches!(
                finding.rule_id,
                InternalErrorComplianceId::SourceShape001
                    | InternalErrorComplianceId::SourceTrackCaller001
            )
        }),
        "location: &'static Location must satisfy the shape: {:?}",
        report.compliance.findings
    );
    Ok(())
}

#[test]
fn constructor_without_track_caller_is_a_violation() -> miette::Result<()> {
    let fixture = write_error_crate(MISSING_TRACK_CALLER_SOURCE)?;
    let report = scan_crate_internal_error_chain(fixture.path(), "fixture")
        .into_diagnostic()
        .wrap_err("scan")?;
    assert!(
        report.compliance.findings.iter().any(|finding| {
            finding.rule_id == InternalErrorComplianceId::SourceTrackCaller001
                && finding.context.contains("IoSource")
        }),
        "new(source, file, line) without #[track_caller] must be flagged: {:?}",
        report.compliance.findings
    );
    assert!(
        !report
            .compliance
            .findings
            .iter()
            .any(|finding| finding.rule_id == InternalErrorComplianceId::SourceShape001),
        "file+line fields are present: {:?}",
        report.compliance.findings
    );
    Ok(())
}

#[test]
fn from_impl_without_track_caller_is_a_violation() -> miette::Result<()> {
    let src = r#"
use std::io;
use std::panic::Location;

pub struct Error {
    kind: Box<ErrorKind>,
}

pub enum ErrorKind {
    Io(IoSource),
}

pub struct IoSource {
    source: io::Error,
    file: &'static str,
    line: u32,
}

impl IoSource {
    #[track_caller]
    pub fn new(source: io::Error) -> Self {
        let loc = Location::caller();
        Self { source, file: loc.file(), line: loc.line() }
    }
}

impl From<io::Error> for IoSource {
    fn from(source: io::Error) -> Self {
        Self::new(source)
    }
}

impl std::error::Error for Error {}
impl std::error::Error for IoSource {}
"#;
    let fixture = write_error_crate(src)?;
    let report = scan_crate_internal_error_chain(fixture.path(), "fixture")
        .into_diagnostic()
        .wrap_err("scan")?;
    assert!(
        report.compliance.findings.iter().any(|finding| {
            finding.rule_id == InternalErrorComplianceId::SourceTrackCaller001
                && finding
                    .internal_constructor
                    .as_deref()
                    .is_some_and(|name| name.contains("From"))
        }),
        "From::from without #[track_caller] must be flagged: {:?}",
        report.compliance.findings
    );
    Ok(())
}

#[test]
fn from_alone_is_not_a_substitute_for_new() -> miette::Result<()> {
    let src = r#"
use std::io;
use std::panic::Location;

pub struct Error {
    kind: Box<ErrorKind>,
}

pub enum ErrorKind {
    Io(IoSource),
}

pub struct IoSource {
    source: io::Error,
    file: &'static str,
    line: u32,
}

impl From<io::Error> for IoSource {
    #[track_caller]
    fn from(source: io::Error) -> Self {
        let loc = Location::caller();
        Self { source, file: loc.file(), line: loc.line() }
    }
}

impl std::error::Error for Error {}
impl std::error::Error for IoSource {}
"#;
    let fixture = write_error_crate(src)?;
    let report = scan_crate_internal_error_chain(fixture.path(), "fixture")
        .into_diagnostic()
        .wrap_err("scan")?;
    assert!(
        report.compliance.findings.iter().any(|finding| {
            finding.rule_id == InternalErrorComplianceId::SourceTrackCaller001
                && finding.context.contains("IoSource")
                && finding.snippet.contains("fn new")
        }),
        "From::from with Location::caller() must not stand in for new: {:?}",
        report.compliance.findings
    );
    Ok(())
}

#[test]
fn new_must_not_take_file_and_line_args() -> miette::Result<()> {
    let src = r#"
use std::io;
use std::panic::Location;

pub struct Error {
    kind: Box<ErrorKind>,
}

pub enum ErrorKind {
    Io(IoSource),
}

pub struct IoSource {
    source: io::Error,
    file: &'static str,
    line: u32,
}

impl IoSource {
    #[track_caller]
    pub fn new(source: io::Error, file: &'static str, line: u32) -> Self {
        let _ = Location::caller();
        Self { source, file, line }
    }
}

impl std::error::Error for Error {}
impl std::error::Error for IoSource {}
"#;
    let fixture = write_error_crate(src)?;
    let report = scan_crate_internal_error_chain(fixture.path(), "fixture")
        .into_diagnostic()
        .wrap_err("scan")?;
    assert!(
        report.compliance.findings.iter().any(|finding| {
            finding.rule_id == InternalErrorComplianceId::SourceTrackCaller001
                && finding
                    .internal_constructor
                    .as_deref()
                    .is_some_and(|name| name == "new")
                && finding.snippet.contains("must not take file/line/location")
        }),
        "new(source, file, line) must be flagged even with Location::caller(): {:?}",
        report.compliance.findings
    );
    Ok(())
}

#[test]
fn parent_from_without_track_caller_is_a_violation() -> miette::Result<()> {
    let src = r#"
use std::io;
use std::panic::Location;

pub struct Error {
    kind: Box<ErrorKind>,
}

pub enum ErrorKind {
    Io(IoSource),
}

pub struct IoSource {
    source: io::Error,
    file: &'static str,
    line: u32,
}

impl IoSource {
    #[track_caller]
    pub fn new(source: io::Error) -> Self {
        let loc = Location::caller();
        Self { source, file: loc.file(), line: loc.line() }
    }
}

impl From<io::Error> for Error {
    fn from(source: io::Error) -> Self {
        Self { kind: Box::new(ErrorKind::Io(IoSource::new(source))) }
    }
}

impl std::error::Error for Error {}
impl std::error::Error for IoSource {}
"#;
    let fixture = write_error_crate(src)?;
    let report = scan_crate_internal_error_chain(fixture.path(), "fixture")
        .into_diagnostic()
        .wrap_err("scan")?;
    assert!(
        report.compliance.findings.iter().any(|finding| {
            finding.rule_id == InternalErrorComplianceId::SourceTrackCaller001
                && finding.context.contains("Error")
                && finding
                    .internal_constructor
                    .as_deref()
                    .is_some_and(|name| name.contains("From"))
        }),
        "parent From without #[track_caller] hides the call site: {:?}",
        report.compliance.findings
    );
    Ok(())
}

#[test]
fn error_enum_is_not_a_parent() -> miette::Result<()> {
    let fixture = write_error_crate(ERROR_RS_FIXTURE)?;
    let report = scan_crate_internal_error_chain(fixture.path(), "fixture")
        .into_diagnostic()
        .wrap_err("scan")?;
    assert!(
        report.compliance.findings.iter().any(|finding| {
            finding.rule_id == InternalErrorComplianceId::ArchParent001
                && finding.context.contains("CordialError")
        }),
        "error enum must not stand in for parent+Kind: {:?}",
        report.compliance.findings
    );
    assert!(
        report.compliance.findings.iter().any(|finding| {
            finding.rule_id == InternalErrorComplianceId::ArchKindVariant001
                && finding
                    .foreign_error_type
                    .as_deref()
                    .is_some_and(|ty| ty.contains("io"))
        }),
        "naked foreign variant must be KIND-VARIANT: {:?}",
        report.compliance.findings
    );
    Ok(())
}

#[test]
fn unboxed_kind_field_is_a_violation() -> miette::Result<()> {
    let src = r#"
use std::io;
use std::panic::Location;

pub struct Error {
    kind: ErrorKind,
}

pub enum ErrorKind {
    Io(IoSource),
}

pub struct IoSource {
    source: io::Error,
    file: &'static str,
    line: u32,
}

impl IoSource {
    #[track_caller]
    pub fn new(source: io::Error) -> Self {
        let loc = Location::caller();
        Self { source, file: loc.file(), line: loc.line() }
    }
}

impl std::error::Error for Error {}
impl std::error::Error for IoSource {}
"#;
    let fixture = write_error_crate(src)?;
    let report = scan_crate_internal_error_chain(fixture.path(), "fixture")
        .into_diagnostic()
        .wrap_err("scan")?;
    assert!(
        report
            .compliance
            .findings
            .iter()
            .any(|finding| finding.rule_id == InternalErrorComplianceId::ArchKindBox001),
        "unboxed kind must be KIND-BOX: {:?}",
        report.compliance.findings
    );
    Ok(())
}

#[test]
fn nested_kind_on_native_source_is_accepted() -> miette::Result<()> {
    let src = r#"
use std::io;
use std::panic::Location;

pub struct Error {
    kind: Box<ErrorKind>,
}

pub enum ErrorKind {
    Parse(ParseSource),
}

pub struct ParseSource {
    kind: Box<ParseKind>,
    file: &'static str,
    line: u32,
}

impl ParseSource {
    #[track_caller]
    pub fn new(kind: ParseKind) -> Self {
        let loc = Location::caller();
        Self { kind: Box::new(kind), file: loc.file(), line: loc.line() }
    }
}

pub enum ParseKind {
    Io(IoSource),
}

pub struct IoSource {
    source: io::Error,
    file: &'static str,
    line: u32,
}

impl IoSource {
    #[track_caller]
    pub fn new(source: io::Error) -> Self {
        let loc = Location::caller();
        Self { source, file: loc.file(), line: loc.line() }
    }
}

impl std::error::Error for Error {}
impl std::error::Error for ParseSource {}
impl std::error::Error for IoSource {}
"#;
    let fixture = write_error_crate(src)?;
    let report = scan_crate_internal_error_chain(fixture.path(), "fixture")
        .into_diagnostic()
        .wrap_err("scan")?;
    let arch: Vec<_> = report
        .compliance
        .findings
        .iter()
        .filter(|finding| {
            matches!(
                finding.rule_id,
                InternalErrorComplianceId::ArchParent001
                    | InternalErrorComplianceId::ArchKindBox001
                    | InternalErrorComplianceId::ArchKindVariant001
                    | InternalErrorComplianceId::ArchOrphanSource001
                    | InternalErrorComplianceId::SourceShape001
                    | InternalErrorComplianceId::SourceTrackCaller001
            )
        })
        .collect();
    assert!(
        arch.is_empty(),
        "nested Kind on a native source must be accepted: {arch:?}"
    );
    Ok(())
}

#[test]
fn native_source_beside_call_site_is_connected() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        "pub mod error;\npub mod io;\n",
    )
    .into_diagnostic()
    .wrap_err("write lib")?;
    fs::write(
        fixture.path().join("src/error.rs"),
        r#"
use crate::io::IoSource;

pub struct Error {
    kind: Box<ErrorKind>,
}

pub enum ErrorKind {
    Io(IoSource),
}

impl std::error::Error for Error {}
"#,
    )
    .into_diagnostic()
    .wrap_err("write error")?;
    fs::write(
        fixture.path().join("src/io.rs"),
        r#"
use std::io;
use std::panic::Location;

pub struct IoSource {
    source: io::Error,
    file: &'static str,
    line: u32,
}

impl IoSource {
    #[track_caller]
    pub fn new(source: io::Error) -> Self {
        let loc = Location::caller();
        Self { source, file: loc.file(), line: loc.line() }
    }
}

impl std::error::Error for IoSource {}
"#,
    )
    .into_diagnostic()
    .wrap_err("write io")?;

    let report = scan_crate_internal_error_chain(fixture.path(), "fixture")
        .into_diagnostic()
        .wrap_err("scan")?;
    let arch: Vec<_> = report
        .compliance
        .findings
        .iter()
        .filter(|finding| {
            matches!(
                finding.rule_id,
                InternalErrorComplianceId::ArchParent001
                    | InternalErrorComplianceId::ArchKindBox001
                    | InternalErrorComplianceId::ArchKindVariant001
                    | InternalErrorComplianceId::ArchOrphanSource001
                    | InternalErrorComplianceId::SourceShape001
                    | InternalErrorComplianceId::SourceTrackCaller001
            )
        })
        .collect();
    assert!(
        arch.is_empty(),
        "IoSource next to its call site must connect to parent/Kind: {arch:?}"
    );
    assert!(
        report.type_graph.nodes.iter().any(|node| {
            node.type_path.contains("IoSource")
                && node.node_class == InternalErrorNodeClass::ForeignBridge
        }),
        "type graph must include the out-of-error-module source: {:?}",
        report.type_graph.nodes
    );
    Ok(())
}

#[test]
fn type_without_error_impl_is_not_a_native_source() -> miette::Result<()> {
    let fixture = write_error_crate(
        r#"
use std::io;
use std::panic::Location;

pub struct Error {
    kind: Box<ErrorKind>,
}

pub enum ErrorKind {
    Io(IoSource),
}

pub struct IoSource {
    source: io::Error,
    file: &'static str,
    line: u32,
}

impl IoSource {
    #[track_caller]
    pub fn new(source: io::Error) -> Self {
        let loc = Location::caller();
        Self { source, file: loc.file(), line: loc.line() }
    }
}

pub struct NotAnError {
    source: io::Error,
    file: &'static str,
    line: u32,
}

impl std::error::Error for Error {}
impl std::error::Error for IoSource {}
"#,
    )?;
    let report = scan_crate_internal_error_chain(fixture.path(), "fixture")
        .into_diagnostic()
        .wrap_err("scan")?;
    assert!(
        !report
            .compliance
            .findings
            .iter()
            .any(|finding| finding.context.contains("NotAnError")),
        "structs that do not implement Error are not in the error set: {:?}",
        report.compliance.findings
    );
    Ok(())
}

#[test]
fn dogfood_cordial_follows_error_architecture() -> miette::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let report = scan_crate_internal_error_chain(root, "cordial")
        .into_diagnostic()
        .wrap_err("scan cordial")?;
    let arch: Vec<_> = report
        .compliance
        .findings
        .iter()
        .filter(|finding| {
            matches!(
                finding.rule_id,
                InternalErrorComplianceId::ArchParent001
                    | InternalErrorComplianceId::ArchKindBox001
                    | InternalErrorComplianceId::ArchKindVariant001
                    | InternalErrorComplianceId::ArchOrphanSource001
                    | InternalErrorComplianceId::SourceShape001
                    | InternalErrorComplianceId::SourceTrackCaller001
            )
        })
        .collect();
    assert!(
        arch.is_empty(),
        "cordial should follow parent/Kind/native-source architecture: {arch:?}"
    );
    Ok(())
}

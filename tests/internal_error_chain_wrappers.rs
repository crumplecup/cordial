//! Source wrappers: the required shape, `#[track_caller]` on constructors and `From`, and owned file and line.

use miette::{IntoDiagnostic, WrapErr};
use std::fs;
#[path = "support/error_reports.rs"]
mod error_reports;
use cordial::InternalErrorComplianceId;

#[path = "support/error_chain_write_crate.rs"]
mod error_chain_write_crate;

use error_chain_write_crate::write_error_crate;

const WELL_FORMED_SOURCE: &str = include_str!("fixtures/error_chain/well_formed_source.rs");

const LOCATION_FIELD_SOURCE: &str = include_str!("fixtures/error_chain/location_field_source.rs");

const MISSING_TRACK_CALLER_SOURCE: &str =
    include_str!("fixtures/error_chain/missing_track_caller_source.rs");

const TYPE_GRAPH_FIXTURE: &str = include_str!("fixtures/error_chain/type_graph.rs");

#[test]
fn incomplete_source_wrapper_is_shape_and_track_caller_violation() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src/error"))
        .into_diagnostic()
        .wrap_err("error dir")?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        "mod error {\n    pub mod type_graph;\n}\n",
    )
    .into_diagnostic()?;
    fs::write(
        fixture.path().join("src/error/type_graph.rs"),
        TYPE_GRAPH_FIXTURE,
    )
    .into_diagnostic()
    .wrap_err("write type graph")?;

    let report = error_reports::internal_report(fixture.path(), "fixture")?;
    assert!(
        report.compliance().findings().iter().any(|finding| {
            finding.rule_id() == InternalErrorComplianceId::SourceShape001
                && finding.context().contains("InnerSource")
        }),
        "InnerSource without file/line must be SOURCE-SHAPE: {:?}",
        report.compliance().findings()
    );
    assert!(
        report.compliance().findings().iter().any(|finding| {
            finding.rule_id() == InternalErrorComplianceId::SourceTrackCaller001
                && finding.context().contains("InnerSource")
        }),
        "InnerSource without #[track_caller] constructor must be TRACK-CALLER: {:?}",
        report.compliance().findings()
    );
    Ok(())
}

#[test]
fn well_formed_source_wrapper_is_not_a_compliance_violation() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_error_crate(WELL_FORMED_SOURCE)?;
    let report = error_reports::internal_report(fixture.path(), "fixture")?;
    let source_findings: Vec<_> = report
        .compliance()
        .findings()
        .iter()
        .filter(|finding| {
            matches!(
                finding.rule_id(),
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
        !report.compliance().findings().iter().any(|finding| {
            matches!(
                finding.rule_id(),
                InternalErrorComplianceId::ArchParent001
                    | InternalErrorComplianceId::ArchKindBox001
                    | InternalErrorComplianceId::ArchKindVariant001
                    | InternalErrorComplianceId::ArchOrphanSource001
            )
        }),
        "well-formed parent/Kind/source must not be architecture-flagged: {:?}",
        report.compliance().findings()
    );
    Ok(())
}

#[test]
fn instrumented_return_hides_tracked_constructor_call_site() -> miette::Result<()> {
    cordial::init_tracing();
    let source = WELL_FORMED_SOURCE.replace(
        "    #[track_caller]\n    pub fn new(source: io::Error)",
        "    #[track_caller]\n    #[instrument(ret)]\n    pub fn new(source: io::Error)",
    );
    let fixture = write_error_crate(&source)?;
    let report = error_reports::internal_report(fixture.path(), "fixture")?;
    assert!(report.compliance().findings().iter().any(|finding| {
        finding.rule_id() == InternalErrorComplianceId::SourceTrackCaller001
            && finding.snippet().contains("instrument(ret)")
    }));
    Ok(())
}

#[test]
fn location_field_is_rejected_in_favor_of_owned_file_and_line() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_error_crate(LOCATION_FIELD_SOURCE)?;
    let report = error_reports::internal_report(fixture.path(), "fixture")?;
    assert!(
        report.compliance().findings().iter().any(|finding| {
            finding.rule_id() == InternalErrorComplianceId::SourceShape001
                && finding.context().contains("IoSource")
                && finding.snippet().contains("copy owned `file` and `line`")
        }),
        "location: &'static Location must be SOURCE-SHAPE: {:?}",
        report.compliance().findings()
    );
    Ok(())
}

#[test]
fn constructor_without_track_caller_is_a_violation() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_error_crate(MISSING_TRACK_CALLER_SOURCE)?;
    let report = error_reports::internal_report(fixture.path(), "fixture")?;
    assert!(
        report.compliance().findings().iter().any(|finding| {
            finding.rule_id() == InternalErrorComplianceId::SourceTrackCaller001
                && finding.context().contains("IoSource")
        }),
        "new(source, file, line) without #[track_caller] must be flagged: {:?}",
        report.compliance().findings()
    );
    assert!(
        !report
            .compliance()
            .findings()
            .iter()
            .any(|finding| finding.rule_id() == InternalErrorComplianceId::SourceShape001),
        "file+line fields are present: {:?}",
        report.compliance().findings()
    );
    Ok(())
}

#[test]
fn from_impl_without_track_caller_is_a_violation() -> miette::Result<()> {
    cordial::init_tracing();
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
    let report = error_reports::internal_report(fixture.path(), "fixture")?;
    assert!(
        report.compliance().findings().iter().any(|finding| {
            finding.rule_id() == InternalErrorComplianceId::SourceTrackCaller001
                && finding
                    .internal_constructor()
                    .as_deref()
                    .is_some_and(|name| name.contains("From"))
        }),
        "From::from without #[track_caller] must be flagged: {:?}",
        report.compliance().findings()
    );
    Ok(())
}

#[test]
fn from_alone_is_not_a_substitute_for_new() -> miette::Result<()> {
    cordial::init_tracing();
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
    let report = error_reports::internal_report(fixture.path(), "fixture")?;
    assert!(
        report.compliance().findings().iter().any(|finding| {
            finding.rule_id() == InternalErrorComplianceId::SourceTrackCaller001
                && finding.context().contains("IoSource")
                && finding.snippet().contains("fn new")
        }),
        "From::from with Location::caller() must not stand in for new: {:?}",
        report.compliance().findings()
    );
    Ok(())
}

#[test]
fn new_must_not_take_file_and_line_args() -> miette::Result<()> {
    cordial::init_tracing();
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
    let report = error_reports::internal_report(fixture.path(), "fixture")?;
    assert!(
        report.compliance().findings().iter().any(|finding| {
            finding.rule_id() == InternalErrorComplianceId::SourceTrackCaller001
                && finding
                    .internal_constructor()
                    .as_deref()
                    .is_some_and(|name| name == "new")
                && finding
                    .snippet()
                    .contains("must not take file/line/location")
        }),
        "new(source, file, line) must be flagged even with Location::caller(): {:?}",
        report.compliance().findings()
    );
    Ok(())
}

#[test]
fn parent_from_without_track_caller_is_a_violation() -> miette::Result<()> {
    cordial::init_tracing();
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
    let report = error_reports::internal_report(fixture.path(), "fixture")?;
    assert!(
        report.compliance().findings().iter().any(|finding| {
            finding.rule_id() == InternalErrorComplianceId::SourceTrackCaller001
                && finding.context().contains("Error")
                && finding
                    .internal_constructor()
                    .as_deref()
                    .is_some_and(|name| name.contains("From"))
        }),
        "parent From without #[track_caller] hides the call site: {:?}",
        report.compliance().findings()
    );
    Ok(())
}

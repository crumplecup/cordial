//! Kind fields and native sources: the borrowed strategy, boxed kinds, nested kinds, and native source call sites.

use miette::{IntoDiagnostic, WrapErr};
use std::fs;
use std::path::Path;
#[path = "support/error_reports.rs"]
mod error_reports;
use cordial::{
    INTERNAL_ERROR_CHAIN_ETIQUETTE, InternalErrorComplianceId, InternalErrorNodeClass, RunAll,
    Session, SessionBuilder,
};

#[path = "support/error_chain_write_crate.rs"]
mod error_chain_write_crate;

use error_chain_write_crate::write_error_crate;

const WELL_FORMED_SOURCE: &str = include_str!("fixtures/error_chain/well_formed_source.rs");

const ERROR_RS_FIXTURE: &str = include_str!("fixtures/error_chain/error_rs.rs");

fn scan_error_with_strategy(src: &str, strategy: &str) -> miette::Result<String> {
    let fixture = write_error_crate(src)?;
    fs::write(
        fixture.path().join("cordial.toml"),
        format!("[internal_error_chain]\nfield_strategy = \"{strategy}\"\n"),
    )
    .into_diagnostic()?;
    let store = tempfile::tempdir().into_diagnostic()?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&*INTERNAL_ERROR_CHAIN_ETIQUETTE)
        .build();
    session.run(&RunAll).into_diagnostic()?;
    fs::read_to_string(store.path().join("findings/internal-error-compliance.csv"))
        .into_diagnostic()
}

#[test]
fn borrowed_strategy_checks_every_non_copy_error_field_and_kind() -> miette::Result<()> {
    cordial::init_tracing();
    let source =
        WELL_FORMED_SOURCE.replace("    line: u32,", "    line: u32,\n    code: LocalCode,");
    let source = format!("#[derive(Clone, Copy)]\nstruct LocalCode(u32);\n{source}");
    let csv = scan_error_with_strategy(&source, "borrowed")?;
    assert!(
        csv.contains("ERROR-CHAIN-COMPLIANCE-ARCH-KIND-BOX-001"),
        "{csv}"
    );
    for field in ["IoSource::source", "IoSource::file", "ErrorKind::Io::_0"] {
        assert!(csv.contains(field), "missing {field}: {csv}");
    }
    assert!(!csv.contains("IoSource::line"), "{csv}");
    assert!(!csv.contains("IoSource::code"), "{csv}");
    assert!(
        !csv.contains("Error::kind,ERROR-CHAIN-COMPLIANCE-FIELD-STRATEGY-001"),
        "{csv}"
    );
    Ok(())
}

#[test]
fn borrowed_strategy_accepts_borrowed_kind_source_and_file() -> miette::Result<()> {
    cordial::init_tracing();
    let source = r#"
use std::io;
pub struct Error<'a> { kind: &'a ErrorKind<'a> }
pub enum ErrorKind<'a> { Io(&'a IoSource<'a>) }
pub struct IoSource<'a> {
    source: &'a io::Error,
    file: &'static str,
    line: u32,
}
impl std::fmt::Display for Error<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "error") }
}
impl std::fmt::Debug for Error<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "error") }
}
impl std::fmt::Display for IoSource<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "io") }
}
impl std::fmt::Debug for IoSource<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "io") }
}
impl std::error::Error for Error<'_> {}
impl std::error::Error for IoSource<'_> {}
"#;
    let csv = scan_error_with_strategy(source, "borrowed")?;
    assert!(
        !csv.contains("ERROR-CHAIN-COMPLIANCE-FIELD-STRATEGY-001"),
        "{csv}"
    );
    assert!(
        !csv.contains("ERROR-CHAIN-COMPLIANCE-ARCH-KIND-BOX-001"),
        "{csv}"
    );
    assert!(
        !csv.contains("ERROR-CHAIN-COMPLIANCE-ARCH-KIND-VARIANT-001"),
        "{csv}"
    );
    Ok(())
}

#[test]
fn error_enum_is_not_a_parent() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_error_crate(ERROR_RS_FIXTURE)?;
    let report = error_reports::internal_report(fixture.path(), "fixture")?;
    assert!(
        report.compliance().findings().iter().any(|finding| {
            finding.rule_id() == InternalErrorComplianceId::ArchParent001
                && finding.context().contains("CordialError")
        }),
        "error enum must not stand in for parent+Kind: {:?}",
        report.compliance().findings()
    );
    assert!(
        report.compliance().findings().iter().any(|finding| {
            finding.rule_id() == InternalErrorComplianceId::ArchKindVariant001
                && finding
                    .foreign_error_type()
                    .as_deref()
                    .is_some_and(|ty| ty.contains("io"))
        }),
        "naked foreign variant must be KIND-VARIANT: {:?}",
        report.compliance().findings()
    );
    Ok(())
}

#[test]
fn unboxed_kind_field_is_a_violation() -> miette::Result<()> {
    cordial::init_tracing();
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
    let report = error_reports::internal_report(fixture.path(), "fixture")?;
    assert!(
        report
            .compliance()
            .findings()
            .iter()
            .any(|finding| finding.rule_id() == InternalErrorComplianceId::ArchKindBox001),
        "unboxed kind must be KIND-BOX: {:?}",
        report.compliance().findings()
    );
    Ok(())
}

#[test]
fn nested_kind_on_native_source_is_accepted() -> miette::Result<()> {
    cordial::init_tracing();
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
    let report = error_reports::internal_report(fixture.path(), "fixture")?;
    let arch: Vec<_> = report
        .compliance()
        .findings()
        .iter()
        .filter(|finding| {
            matches!(
                finding.rule_id(),
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

    let report = error_reports::internal_report(fixture.path(), "fixture")?;
    let arch: Vec<_> = report
        .compliance()
        .findings()
        .iter()
        .filter(|finding| {
            matches!(
                finding.rule_id(),
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
        report.type_graph().nodes().iter().any(|node| {
            node.type_path().contains("IoSource")
                && node.node_class() == InternalErrorNodeClass::ForeignBridge
        }),
        "type graph must include the out-of-error-module source: {:?}",
        report.type_graph().nodes()
    );
    Ok(())
}

#[test]
fn type_without_error_impl_is_not_a_native_source() -> miette::Result<()> {
    cordial::init_tracing();
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
    let report = error_reports::internal_report(fixture.path(), "fixture")?;
    assert!(
        !report
            .compliance()
            .findings()
            .iter()
            .any(|finding| finding.context().contains("NotAnError")),
        "structs that do not implement Error are not in the error set: {:?}",
        report.compliance().findings()
    );
    Ok(())
}

#[test]
fn dogfood_cordial_follows_error_architecture() -> miette::Result<()> {
    cordial::init_tracing();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let report = error_reports::internal_report(root, "cordial")?;
    let arch: Vec<_> = report
        .compliance()
        .findings()
        .iter()
        .filter(|finding| {
            matches!(
                finding.rule_id(),
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

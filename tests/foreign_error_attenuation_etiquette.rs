use miette::{IntoDiagnostic, WrapErr};
use std::fs;
use std::path::PathBuf;

use cordial::{
    ErrorSiteKind, ErrorSiteScanRow, FOREIGN_ERROR_ATTENUATION_ETIQUETTE,
    ForeignErrorHandlingClass, ForeignErrorTypeRecord, ForeignErrorTypeReport,
    ForeignTypeConfidence, RunAll, Session, SessionBuilder, build_error_site_partition_report,
    build_foreign_error_attenuation_report, build_foreign_error_type_report,
    partition_error_site_records, scan_crate_error_chain, scan_error_chain_rust_source,
    scan_error_sites_rust_source,
};

const PRESERVED_FIXTURE: &str = r#"
use std::io;

#[derive(Debug)]
struct IoSource {
    source: io::Error,
}

#[derive(Debug)]
enum UmbrellaKind {
    Io(IoSource),
}

impl From<io::Error> for IoSource {
    fn from(source: io::Error) -> Self {
        Self { source }
    }
}

impl From<IoSource> for UmbrellaKind {
    fn from(value: IoSource) -> Self {
        Self::Io(value)
    }
}

fn preserved_direct() -> Result<(), UmbrellaKind> {
    std::fs::read_to_string("x")?;
    Ok(())
}

fn preserved_map_err_from() -> Result<(), UmbrellaKind> {
    std::fs::read_to_string("x").map_err(IoSource::from)?;
    Ok(())
}

fn preserved_map_err_into() -> Result<(), UmbrellaKind> {
    std::fs::read_to_string("x").map_err(Into::into)?;
    Ok(())
}

fn preserved_map_err_closure() -> Result<(), UmbrellaKind> {
    std::fs::read_to_string("x").map_err(|e| UmbrellaKind::Io(IoSource { source: e }))?;
    Ok(())
}

fn broken_stringify() -> Result<(), UmbrellaKind> {
    std::fs::read_to_string("x").map_err(|e| UmbrellaKind::Io(IoSource {
        source: io::Error::new(io::ErrorKind::Other, e.to_string()),
    }))?;
    Ok(())
}
"#;

fn scan_sites_fixture() -> miette::Result<Vec<ErrorSiteScanRow>> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let file = fixture.path().join("preserved.rs");
    fs::write(&file, PRESERVED_FIXTURE)
        .into_diagnostic()
        .wrap_err("write fixture")?;
    scan_error_sites_rust_source(PRESERVED_FIXTURE, &file, fixture.path(), fixture.path())
        .into_diagnostic()
        .wrap_err("scan")
        .map(|records| {
            records
                .into_iter()
                .map(|record| ErrorSiteScanRow {
                    crate_name: "fixture".to_string(),
                    kind: record.kind,
                    context: record.context,
                    file: record.file,
                    line: record.line,
                    source_snippet: record.source_snippet,
                    site_snippet: record.site_snippet,
                })
                .collect()
        })
}

fn scan_chain_fixture() -> miette::Result<Vec<cordial::ErrorChainRecord>> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let file = fixture.path().join("preserved.rs");
    fs::write(&file, PRESERVED_FIXTURE)
        .into_diagnostic()
        .wrap_err("write fixture")?;
    scan_error_chain_rust_source(PRESERVED_FIXTURE, &file, fixture.path(), fixture.path())
        .into_diagnostic()
        .wrap_err("scan")
}

#[test]
fn attenuator_pairs_preserved_and_chain_break_sites() -> miette::Result<()> {
    cordial::init_tracing();
    let scan_rows = scan_sites_fixture()?;
    let partition_rows = partition_error_site_records(&scan_rows, "fixture");
    let partition = build_error_site_partition_report("fixture", partition_rows);
    let foreign = build_foreign_error_type_report(&partition);
    let chain = scan_chain_fixture()?;
    let attenuation = build_foreign_error_attenuation_report(&foreign, &chain);

    assert!(attenuation.findings.iter().any(|finding| {
        finding.handling_class == ForeignErrorHandlingClass::ChainPreserved
            && finding.context.contains("preserved_direct")
    }));
    assert!(attenuation.findings.iter().any(|finding| {
        finding.handling_class == ForeignErrorHandlingClass::ChainBreak
            && finding.context.contains("broken_stringify")
            && finding.bad_pattern.contains("map_err")
            && finding.good_pattern.contains("from")
    }));
    assert!(!attenuation.findings.iter().any(|finding| {
        finding.handling_class == ForeignErrorHandlingClass::ChainBreak
            && finding.context.contains("preserved_map_err_from")
    }));
    Ok(())
}

#[test]
fn test_into_diagnostic_is_miette_exemplar_not_pending_infra() {
    cordial::init_tracing();
    let foreign = ForeignErrorTypeReport {
        crate_name: "example".to_string(),
        findings: vec![ForeignErrorTypeRecord {
            crate_name: "example".to_string(),
            foreign_error_type: "std::io::Error".to_string(),
            rule_id: "FOREIGN-ERROR-TYPE-STD-IO-FS-001".to_string(),
            confidence: ForeignTypeConfidence::High,
            chain_break: false,
            kind: ErrorSiteKind::QuestionMark,
            context: "register::three_plugin_kinds_register_and_quality_finds_todo".to_string(),
            file: PathBuf::from("tests/custom_plugins.rs"),
            line: 25,
            source_snippet: "std::fs::create_dir_all(…).into_diagnostic(…).wrap_err(…)".to_string(),
            site_snippet: "std::fs::create_dir_all(…).into_diagnostic(…).wrap_err(…)?".to_string(),
        }],
    };
    let report = build_foreign_error_attenuation_report(&foreign, &[]);
    assert_eq!(report.findings.len(), 1);
    assert_eq!(
        report.findings[0].handling_class,
        ForeignErrorHandlingClass::ChainPreserved
    );
}

#[test]
fn display_fmt_question_mark_is_exemplar_not_pending_infra() {
    cordial::init_tracing();
    let foreign = ForeignErrorTypeReport {
        crate_name: "example".to_string(),
        findings: vec![ForeignErrorTypeRecord {
            crate_name: "example".to_string(),
            foreign_error_type: "std::fmt::Error".to_string(),
            rule_id: "FOREIGN-ERROR-TYPE-STD-FMT-001".to_string(),
            confidence: ForeignTypeConfidence::High,
            chain_break: false,
            kind: ErrorSiteKind::QuestionMark,
            context: "provenance_test::ManualCertificate::fmt".to_string(),
            file: PathBuf::from("tests/provenance_test.rs"),
            line: 71,
            source_snippet: "write!(…)".to_string(),
            site_snippet: "write!(…)?".to_string(),
        }],
    };
    let report = build_foreign_error_attenuation_report(&foreign, &[]);
    assert_eq!(report.findings.len(), 1);
    assert_eq!(
        report.findings[0].handling_class,
        ForeignErrorHandlingClass::ChainPreserved
    );
}

#[test]
fn library_into_diagnostic_without_bridge_is_still_pending_infra() {
    cordial::init_tracing();
    let foreign = ForeignErrorTypeReport {
        crate_name: "example".to_string(),
        findings: vec![ForeignErrorTypeRecord {
            crate_name: "example".to_string(),
            foreign_error_type: "std::io::Error".to_string(),
            rule_id: "FOREIGN-ERROR-TYPE-STD-IO-FS-001".to_string(),
            confidence: ForeignTypeConfidence::High,
            chain_break: false,
            kind: ErrorSiteKind::QuestionMark,
            context: "lib::load".to_string(),
            file: PathBuf::from("src/lib.rs"),
            line: 10,
            source_snippet: "std::fs::read_to_string(…).into_diagnostic()".to_string(),
            site_snippet: "std::fs::read_to_string(…).into_diagnostic()?".to_string(),
        }],
    };
    let report = build_foreign_error_attenuation_report(&foreign, &[]);
    assert_eq!(
        report.findings[0].handling_class,
        ForeignErrorHandlingClass::PendingInfrastructure
    );
}

#[test]
fn chain_break_rows_carry_baked_in_resolution() -> miette::Result<()> {
    cordial::init_tracing();
    let scan_rows = scan_sites_fixture()?;
    let partition_rows = partition_error_site_records(&scan_rows, "fixture");
    let partition = build_error_site_partition_report("fixture", partition_rows);
    let foreign = build_foreign_error_type_report(&partition);
    let chain = scan_chain_fixture()?;
    let attenuation = build_foreign_error_attenuation_report(&foreign, &chain);
    let broken = attenuation
        .findings
        .iter()
        .find(|finding| finding.context.contains("broken_stringify"))
        .ok_or_else(|| miette::miette!("broken site"))?;
    assert!(
        broken.resolution.contains("newtype") || broken.resolution.contains("CrateError"),
        "resolution should name a crate error newtype, got: {}",
        broken.resolution
    );
    assert!(broken.good_pattern.contains("from"));
    Ok(())
}

#[test]
fn foreign_error_attenuation_session_produces_csv() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(fixture.path().join("src/lib.rs"), PRESERVED_FIXTURE)
        .into_diagnostic()
        .wrap_err("write fixture")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&FOREIGN_ERROR_ATTENUATION_ETIQUETTE)
        .build();

    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    assert!(outcome.findings().count() >= 1);

    let findings_dir = store.path().join("findings");
    let csv = fs::read_to_string(findings_dir.join("foreign-error-attenuation.csv"))
        .into_diagnostic()
        .wrap_err("foreign-error-attenuation csv")?;
    assert!(csv.contains("ERROR-HANDLING-CHAIN-BREAK"));
    assert!(csv.contains("ERROR-RESOLUTION-REPLACE-STRINGIFY-MAP-ERR"));

    let checklist = fs::read_to_string(findings_dir.join("foreign-error-attenuation.checklist.md"))
        .into_diagnostic()
        .wrap_err("checklist")?;
    assert!(checklist.contains("good:"));
    assert!(checklist.contains("bad:"));

    let summary = fs::read_to_string(findings_dir.join("foreign-error-attenuation-summary.md"))
        .into_diagnostic()
        .wrap_err("summary")?;
    assert!(summary.contains("Preservation rate"));
    Ok(())
}

#[test]
fn scan_crate_error_chain_accepts_fixture_root() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(fixture.path().join("src/lib.rs"), PRESERVED_FIXTURE)
        .into_diagnostic()
        .wrap_err("write fixture")?;
    let records = scan_crate_error_chain(fixture.path())
        .into_diagnostic()
        .wrap_err("scan crate")?;
    assert!(!records.is_empty());
    Ok(())
}

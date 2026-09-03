use miette::{IntoDiagnostic, WrapErr};
use std::fs;

use cordial::{
    ERROR_SITES_ETIQUETTE, ErrorOriginClass, ErrorSiteKind, ErrorSiteScanRow, RunAll, Session,
    SessionBuilder, partition_error_site_row, scan_error_sites_rust_source,
};

fn scan_row(
    kind: ErrorSiteKind,
    context: &str,
    file: &str,
    line: u32,
    source_snippet: &str,
    site_snippet: &str,
) -> ErrorSiteScanRow {
    ErrorSiteScanRow::builder()
        .crate_name("fixture".to_string())
        .kind(kind)
        .context(context.to_string())
        .file(std::path::PathBuf::from(file))
        .line(line)
        .source_snippet(source_snippet.to_string())
        .site_snippet(site_snippet.to_string())
        .build()
        .expect("scan row")
}

const ERROR_SITES: &str = r#"
use std::error::Error;

use cordial::{CordialError, CordialResult};

fn foreign_map_err() -> CordialResult<()> {
    std::fs::read_to_string("x").map_err(CordialError::from)?;
    Ok(())
}

fn propagate_internal(x: CordialResult<()>) -> CordialResult<()> {
    x?;
    Ok(())
}

fn return_internal() -> CordialResult<()> {
    return Err(CordialError::invariant("bad"));
}

fn if_let_foreign(r: Result<i32, std::io::Error>) -> CordialResult<()> {
    if let Err(e) = r {
        return Err(CordialError::from(e));
    }
    Ok(())
}

fn match_foreign(r: Result<i32, Box<dyn Error>>) -> CordialResult<()> {
    match r {
        Err(e) => return Err(CordialError::invariant(e.to_string())),
        Ok(_) => Ok(()),
    }
}

fn option_to_err(x: Option<i32>) -> CordialResult<i32> {
    x.ok_or_else(|| CordialError::invariant("missing"))
}
"#;

#[test]
fn error_site_kinds_are_detected() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let file = fixture.path().join("error_sites.rs");
    fs::write(&file, ERROR_SITES)
        .into_diagnostic()
        .wrap_err("write sample")?;

    let findings = scan_error_sites_rust_source(
        &fs::read_to_string(&file).into_diagnostic()?,
        &file,
        fixture.path(),
        fixture.path(),
    )
    .into_diagnostic()
    .wrap_err("scan")?;

    assert_eq!(findings.len(), 9);
    assert!(findings.iter().any(|f| f.kind() == ErrorSiteKind::MapErr));
    assert!(
        findings
            .iter()
            .any(|f| f.kind() == ErrorSiteKind::QuestionMark)
    );
    assert!(
        findings
            .iter()
            .any(|f| f.kind() == ErrorSiteKind::ReturnErr)
    );
    assert!(findings.iter().any(|f| f.kind() == ErrorSiteKind::IfLetErr));
    assert!(findings.iter().any(|f| f.kind() == ErrorSiteKind::MatchErr));
    assert!(findings.iter().any(|f| f.kind() == ErrorSiteKind::OkOr));
    Ok(())
}

#[test]
fn error_site_context_includes_enclosing_fn() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let file = fixture.path().join("error_sites.rs");
    fs::write(&file, ERROR_SITES)
        .into_diagnostic()
        .wrap_err("write sample")?;

    let findings = scan_error_sites_rust_source(
        &fs::read_to_string(&file).into_diagnostic()?,
        &file,
        fixture.path(),
        fixture.path(),
    )
    .into_diagnostic()
    .wrap_err("scan")?;

    let map_err = findings
        .iter()
        .find(|f| f.kind() == ErrorSiteKind::MapErr)
        .ok_or_else(|| miette::miette!("map_err finding"))?;
    assert!(map_err.context().contains("foreign_map_err"));
    assert!(map_err.source_snippet().contains("read_to_string"));
    Ok(())
}

#[test]
fn error_sites_etiquette_session_produces_csv() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(fixture.path().join("src/lib.rs"), ERROR_SITES)
        .into_diagnostic()
        .wrap_err("write fixture")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&ERROR_SITES_ETIQUETTE)
        .build();

    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    assert_eq!(outcome.findings().count(), 9);

    let findings_dir = store.path().join("findings");
    let csv = fs::read_to_string(findings_dir.join("error-sites.csv"))
        .into_diagnostic()
        .wrap_err("error-sites csv")?;
    assert!(csv.contains("ERROR-SITE-MAP-ERR"));
    assert!(csv.contains("ERROR-SITE-QUESTION-MARK"));
    assert!(csv.contains("ERROR-SITE-RETURN-ERR"));
    assert!(csv.contains("ERROR-SITE-IF-LET-ERR"));
    assert!(csv.contains("ERROR-SITE-MATCH-ERR"));
    assert!(csv.contains("ERROR-SITE-OK-OR"));
    assert!(csv.contains("foreign_map_err"));

    let checklist = fs::read_to_string(findings_dir.join("error-sites.checklist.md"))
        .into_diagnostic()
        .wrap_err("checklist")?;
    assert!(checklist.contains("intermediate"));
    assert!(checklist.contains("- [ ]"));

    let summary = fs::read_to_string(findings_dir.join("error-sites-summary.md"))
        .into_diagnostic()
        .wrap_err("summary")?;
    assert!(summary.contains("**9** sites"));

    let partitioned = fs::read_to_string(findings_dir.join("error-sites-partitioned.csv"))
        .into_diagnostic()
        .wrap_err("partitioned csv")?;
    assert!(partitioned.contains("ERROR-ORIGIN-OTHER"));
    assert!(partitioned.contains("ERROR-ORIGIN-INTERNAL"));

    let partition_summary =
        fs::read_to_string(findings_dir.join("error-sites-partition-summary.md"))
            .into_diagnostic()
            .wrap_err("partition summary")?;
    assert!(partition_summary.contains("foreign pool"));
    Ok(())
}

#[test]
fn map_err_on_std_is_other() {
    cordial::init_tracing();

    let row = scan_row(
        ErrorSiteKind::MapErr,
        "sample",
        "sample.rs",
        1,
        "std::fs::read_to_string(…)",
        "std::fs::read_to_string(…).map_err(…)",
    );
    let partitioned = partition_error_site_row(&row, "fixture").expect("partition");
    assert_eq!(partitioned.origin_class(), ErrorOriginClass::Other);
    assert_eq!(partitioned.origin_detail(), "std");
}

#[test]
fn question_mark_after_map_err_is_internal() -> miette::Result<()> {
    cordial::init_tracing();

    let row = scan_row(
        ErrorSiteKind::QuestionMark,
        "sample",
        "sample.rs",
        1,
        "std::fs::read_to_string(…).map_err(…)",
        "std::fs::read_to_string(…).map_err(…)?",
    );
    let partitioned = partition_error_site_row(&row, "fixture").into_diagnostic()?;
    assert_eq!(partitioned.origin_class(), ErrorOriginClass::Internal);
    assert_eq!(partitioned.origin_detail(), "CordialResult");
    Ok(())
}

#[test]
fn partition_fixture_has_foreign_pool() -> miette::Result<()> {
    cordial::init_tracing();

    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let file = fixture.path().join("error_sites.rs");
    fs::write(&file, ERROR_SITES)
        .into_diagnostic()
        .wrap_err("write sample")?;

    let records = scan_error_sites_rust_source(
        &fs::read_to_string(&file).into_diagnostic()?,
        &file,
        fixture.path(),
        fixture.path(),
    )
    .into_diagnostic()
    .wrap_err("scan")?;

    let mut internal = 0usize;
    let mut other = 0usize;
    for record in records {
        let row = ErrorSiteScanRow::builder()
            .crate_name("fixture".to_string())
            .kind(record.kind())
            .context(record.context().clone())
            .file(record.file().clone())
            .line(record.line())
            .source_snippet(record.source_snippet().clone())
            .site_snippet(record.site_snippet().clone())
            .build()
            .expect("scan row");
        let partitioned = partition_error_site_row(&row, "fixture").into_diagnostic()?;
        match partitioned.origin_class() {
            ErrorOriginClass::Internal => internal += 1,
            ErrorOriginClass::Other => other += 1,
            ErrorOriginClass::Edge => {}
        }
    }
    assert!(other >= 1);
    assert!(internal >= 1);
    Ok(())
}

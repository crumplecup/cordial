use miette::{IntoDiagnostic, WrapErr};
use std::fs;

use cordial::{
    ErrorSiteScanRow, FOREIGN_ERROR_TYPES_ETIQUETTE, RunAll, Session, SessionBuilder,
    build_error_site_partition_report, build_foreign_error_type_report,
    partition_error_site_records, scan_error_sites_rust_source,
};

const ERROR_SITES: &str = r#"
use std::error::Error;

use cordial::{CordialError, CordialResult};

fn foreign_map_err() -> CordialResult<()> {
    std::fs::read_to_string("x").map_err(CordialError::from)?;
    Ok(())
}

fn stringify_map_err() -> CordialResult<()> {
    std::fs::read_to_string("x").map_err(|e| CordialError::invariant(e.to_string()))?;
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

fn scan_fixture() -> miette::Result<Vec<ErrorSiteScanRow>> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let file = fixture.path().join("error_sites.rs");
    fs::write(&file, ERROR_SITES)
        .into_diagnostic()
        .wrap_err("write sample")?;
    scan_error_sites_rust_source(
        &fs::read_to_string(&file).into_diagnostic()?,
        &file,
        fixture.path(),
        fixture.path(),
    )
    .into_diagnostic()
    .wrap_err("scan")
    .map(|records| {
        records
            .into_iter()
            .map(|record| {
                ErrorSiteScanRow::builder()
                    .crate_name("fixture".to_string())
                    .kind(record.kind())
                    .context(record.context().clone())
                    .file(record.file().clone())
                    .line(record.line())
                    .source_snippet(record.source_snippet().clone())
                    .site_snippet(record.site_snippet().clone())
                    .build()
                    .expect("scan row")
            })
            .collect()
    })
}

#[test]
fn foreign_error_types_detect_chain_breaks() -> miette::Result<()> {
    cordial::init_tracing();
    let scan_rows = scan_fixture()?;
    let partition_rows = partition_error_site_records(&scan_rows, "fixture").into_diagnostic()?;
    let partition = build_error_site_partition_report("fixture", partition_rows);
    let foreign_types = build_foreign_error_type_report(&partition).into_diagnostic()?;
    let io_breaks = foreign_types
        .findings()
        .iter()
        .filter(|finding| finding.chain_break() && finding.foreign_error_type() == "std::io::Error")
        .count();
    assert!(
        io_breaks >= 1,
        "without the chain layer this helper treats every map_err as a break candidate"
    );
    Ok(())
}

const SYN_PARSE_SOURCE: &str = r#"
pub fn expand(input: proc_macro2::TokenStream) -> syn::Result<proc_macro2::TokenStream> {
    let item: syn::ItemFn = syn::parse2(input)?;
    Ok(quote::quote!(#item))
}
"#;

fn run_syn_parse_fixture(manifest: Option<&str>) -> miette::Result<String> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(fixture.path().join("src/lib.rs"), SYN_PARSE_SOURCE)
        .into_diagnostic()
        .wrap_err("write fixture")?;
    if let Some(manifest) = manifest {
        fs::write(fixture.path().join("Cargo.toml"), manifest)
            .into_diagnostic()
            .wrap_err("write manifest")?;
    }

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&FOREIGN_ERROR_TYPES_ETIQUETTE)
        .build();
    session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;

    fs::read_to_string(store.path().join("findings/foreign-error-types.csv"))
        .into_diagnostic()
        .wrap_err("foreign-error-types csv")
}

#[test]
fn proc_macro_crate_does_not_flag_syn_error_as_a_foreign_leak() -> miette::Result<()> {
    cordial::init_tracing();
    let csv = run_syn_parse_fixture(Some(
        "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\n\n[lib]\nproc-macro = true\n",
    ))?;
    assert!(
        !csv.contains("syn::Error"),
        "syn::Error is the idiomatic proc-macro expansion currency, not a foreign leak: {csv}"
    );
    Ok(())
}

#[test]
fn non_proc_macro_crate_still_flags_syn_error_as_a_foreign_leak() -> miette::Result<()> {
    cordial::init_tracing();
    let csv = run_syn_parse_fixture(None)?;
    assert!(
        csv.contains("syn::Error"),
        "an ordinary crate parsing syn should still surface the leak: {csv}"
    );
    Ok(())
}

#[test]
fn foreign_error_types_session_produces_artifacts() -> miette::Result<()> {
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
        .register(&FOREIGN_ERROR_TYPES_ETIQUETTE)
        .build();

    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    assert!(outcome.findings().count() >= 1);

    let findings_dir = store.path().join("findings");
    let csv = fs::read_to_string(findings_dir.join("foreign-error-types.csv"))
        .into_diagnostic()
        .wrap_err("foreign-error-types csv")?;
    assert!(csv.contains("std::io::Error"));
    assert!(csv.contains("true"));

    let checklist = fs::read_to_string(findings_dir.join("foreign-error-types.checklist.md"))
        .into_diagnostic()
        .wrap_err("foreign-error-types checklist")?;
    assert!(checklist.contains("chain breaks"));
    assert!(
        checklist.contains("stringify_map_err"),
        "stringifying map_err should remain a chain break: {checklist}"
    );
    #[cfg(feature = "error_chain")]
    assert!(
        !checklist.contains("foreign_map_err"),
        "map_err(CordialError::from) is the preferred wrap, not a break: {checklist}"
    );

    let summary = fs::read_to_string(findings_dir.join("foreign-error-types-summary.md"))
        .into_diagnostic()
        .wrap_err("foreign-error-types summary")?;
    assert!(summary.contains("chain breaks"));

    let foreign_errors = fs::read_to_string(findings_dir.join("foreign-errors.checklist.md"))
        .into_diagnostic()
        .wrap_err("foreign-errors checklist")?;
    assert!(foreign_errors.contains("Foreign error candidates"));
    Ok(())
}

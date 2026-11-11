//! Adding exceptions: resolving the root, writing canonical rows, and preserving coverage skips.

use miette::{IntoDiagnostic, WrapErr};
use std::fs;
use std::path::PathBuf;

use cordial::{
    CoverageSkipEntry, ExceptionEntry, StoreLayout, add_coverage_skip, add_exception,
    coverage_skip_file_path, exception_file_path, resolve_exceptions_root,
};

#[test]
fn resolve_exceptions_root_joins_relative_paths_to_the_project() {
    cordial::init_tracing();
    let project = PathBuf::from("/repos/elicitation");
    assert_eq!(
        resolve_exceptions_root(&project, std::path::Path::new(".elicit_doc-exceptions")),
        project.join(".elicit_doc-exceptions")
    );
    let absolute = PathBuf::from("/tmp/exceptions");
    assert_eq!(resolve_exceptions_root(&project, &absolute), absolute);
}

#[test]
fn add_exception_writes_and_appends_canonical_quality_file() -> miette::Result<()> {
    cordial::init_tracing();
    let store_root = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let store = StoreLayout::from_root(store_root.path(), "demo");
    let first = add_exception(
        &store,
        "panics",
        "demo",
        ExceptionEntry::new("src/lib.rs", "intentional").with_rule_id("PANIC-SOURCE-PANIC"),
    )
    .into_diagnostic()
    .wrap_err("add first")?;
    assert!(first.inserted());
    assert_eq!(
        first.path(),
        exception_file_path(&store, "panics", "demo").as_path()
    );

    let second = add_exception(
        &store,
        "panics",
        "demo",
        ExceptionEntry::new("src/lib.rs", "also intentional").with_line(12),
    )
    .into_diagnostic()
    .wrap_err("add second")?;
    assert!(second.inserted());

    let body = fs::read_to_string(first.path())
        .into_diagnostic()
        .wrap_err("read exceptions")?;
    assert!(body.contains("PANIC-SOURCE-PANIC"));
    assert!(body.contains("also intentional"));
    assert!(body.contains("\"line\": 12"));
    Ok(())
}

#[test]
fn add_exception_is_idempotent_for_identical_rows() -> miette::Result<()> {
    cordial::init_tracing();
    let store_root = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let store = StoreLayout::from_root(store_root.path(), "demo");
    let entry = ExceptionEntry::new("src/lib.rs", "intentional");
    assert!(
        add_exception(&store, "panics", "demo", entry.clone())
            .into_diagnostic()?
            .inserted()
    );
    let again = add_exception(&store, "panics", "demo", entry).into_diagnostic()?;
    assert!(!again.inserted());
    let parsed: Vec<ExceptionEntry> =
        serde_json::from_str(&fs::read_to_string(again.path()).into_diagnostic()?)
            .into_diagnostic()?;
    assert_eq!(parsed.len(), 1);
    Ok(())
}

#[test]
fn add_exception_rejects_empty_file() {
    cordial::init_tracing();
    let store = StoreLayout::from_root("/tmp/unused-exceptions-store", "demo");
    let err = add_exception(
        &store,
        "panics",
        "demo",
        ExceptionEntry::new("   ", "reason"),
    )
    .expect_err("empty file should fail");
    assert!(err.to_string().contains("file must not be empty"));
}

#[test]
fn add_coverage_skip_preserves_existing_extra_fields() -> miette::Result<()> {
    cordial::init_tracing();
    let store_root = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let store = StoreLayout::from_root(store_root.path(), "demo");
    let path = coverage_skip_file_path(&store, "chrono");
    fs::create_dir_all(path.parent().ok_or_else(|| miette::miette!("parent"))?)
        .into_diagnostic()
        .wrap_err("patches dir")?;
    fs::write(
        &path,
        r#"[{"path":"chrono::DateTime","reason":"existing","verifiers":["kani"]}]"#,
    )
    .into_diagnostic()
    .wrap_err("seed skip")?;

    let outcome = add_coverage_skip(
        &store,
        "chrono",
        CoverageSkipEntry::new("chrono::NaiveDate", "new skip"),
    )
    .into_diagnostic()
    .wrap_err("add skip")?;
    assert!(outcome.inserted());
    let body = fs::read_to_string(&path)
        .into_diagnostic()
        .wrap_err("read skip")?;
    assert!(body.contains("\"verifiers\""));
    assert!(body.contains("chrono::NaiveDate"));
    Ok(())
}

//! `remove_exception`, `update_exception`, and stale-exception detection.

use std::collections::HashMap;
use std::fs;

use miette::{IntoDiagnostic, WrapErr};

use cordial::{
    ExceptionEntry, ExceptionSelector, ExceptionUpdate, PANICS_ETIQUETTE, RunAll, Session,
    SessionBuilder, StoreLayout, add_exception, backup_exception_files, exception_file_path,
    load_exception_files, load_exceptions, remove_exception, stale_exceptions, update_exception,
};

fn demo_store() -> miette::Result<(tempfile::TempDir, StoreLayout)> {
    let root = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let store = StoreLayout::from_root(root.path(), "demo");
    Ok((root, store))
}

fn read_rows(store: &StoreLayout, etiquette: &str) -> miette::Result<Vec<ExceptionEntry>> {
    let body = fs::read_to_string(exception_file_path(store, etiquette, "demo"))
        .into_diagnostic()
        .wrap_err("read exception file")?;
    serde_json::from_str(&body).into_diagnostic()
}

fn seed_two_jiff_rows(store: &StoreLayout) -> miette::Result<()> {
    add_exception(
        store,
        "visibility",
        "demo",
        ExceptionEntry::new("src/ext/jiff/mod.rs", "jiff wrapper")
            .with_rule_id("VISIBILITY-A")
            .with_context("ext::jiff"),
    )
    .into_diagnostic()?;
    add_exception(
        store,
        "visibility",
        "demo",
        ExceptionEntry::new("src/ext/jiff/fmt.rs", "jiff fmt")
            .with_rule_id("VISIBILITY-A")
            .with_context("ext::jiff::fmt_strtime_broken_down_time"),
    )
    .into_diagnostic()?;
    Ok(())
}

#[test]
fn remove_deletes_exactly_the_selected_row() -> miette::Result<()> {
    cordial::init_tracing();
    let (_root, store) = demo_store()?;
    seed_two_jiff_rows(&store)?;

    let selector = ExceptionSelector::default()
        .with_rule_id("VISIBILITY-A")
        .with_context("ext::jiff");
    let outcome = remove_exception(&store, "visibility", "demo", &selector).into_diagnostic()?;
    assert!(!outcome.file_deleted());
    assert_eq!(outcome.removed().reason(), "jiff wrapper");

    let rows = read_rows(&store, "visibility")?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].reason(), "jiff fmt");
    Ok(())
}

#[test]
fn remove_errors_on_ambiguous_selector_without_changing_the_file() -> miette::Result<()> {
    cordial::init_tracing();
    let (_root, store) = demo_store()?;
    seed_two_jiff_rows(&store)?;
    let before =
        fs::read_to_string(exception_file_path(&store, "visibility", "demo")).into_diagnostic()?;

    let selector = ExceptionSelector::default().with_rule_id("VISIBILITY-A");
    let err = remove_exception(&store, "visibility", "demo", &selector)
        .expect_err("two rows match the rule id");
    assert!(err.to_string().contains("2 exceptions"), "{err}");

    let after =
        fs::read_to_string(exception_file_path(&store, "visibility", "demo")).into_diagnostic()?;
    assert_eq!(before, after);
    Ok(())
}

#[test]
fn remove_errors_when_nothing_matches_or_selector_is_empty() -> miette::Result<()> {
    cordial::init_tracing();
    let (_root, store) = demo_store()?;
    seed_two_jiff_rows(&store)?;

    let none = ExceptionSelector::default().with_context("jiff::gone");
    let err = remove_exception(&store, "visibility", "demo", &none).expect_err("no match");
    assert!(err.to_string().contains("matches"), "{err}");

    let empty = ExceptionSelector::default();
    let err = remove_exception(&store, "visibility", "demo", &empty).expect_err("empty selector");
    assert!(err.to_string().contains("selector requires"), "{err}");

    let err = remove_exception(
        &store,
        "panics",
        "demo",
        &ExceptionSelector::default().with_line(3u32),
    )
    .expect_err("no file for etiquette");
    assert!(err.to_string().contains("no exceptions"), "{err}");
    assert_eq!(read_rows(&store, "visibility")?.len(), 2);
    Ok(())
}

#[test]
fn removing_the_last_row_deletes_the_file_and_backup_drops_the_stale_copy() -> miette::Result<()> {
    cordial::init_tracing();
    let (_root, store) = demo_store()?;
    let backup = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("backup dir")?;
    add_exception(
        &store,
        "panics",
        "demo",
        ExceptionEntry::new("src/lib.rs", "only row"),
    )
    .into_diagnostic()?;
    backup_exception_files(&store, backup.path()).into_diagnostic()?;
    let backed_up = backup.path().join("demo/exceptions/panics/demo.json");
    assert!(backed_up.is_file());

    let outcome = remove_exception(
        &store,
        "panics",
        "demo",
        &ExceptionSelector::default().with_file("src/lib.rs"),
    )
    .into_diagnostic()?;
    assert!(outcome.file_deleted());
    let path = exception_file_path(&store, "panics", "demo");
    assert!(!path.exists());
    assert!(!path.parent().is_some_and(|dir| dir.exists()));

    backup_exception_files(&store, backup.path()).into_diagnostic()?;
    assert!(!backed_up.exists(), "backup must not keep the removed row");

    // Round trip: load of the (now empty) registry leaves no rows behind.
    fs::create_dir_all(backup.path().join("demo")).into_diagnostic()?;
    load_exception_files(&store, backup.path()).into_diagnostic()?;
    assert!(
        load_exceptions(&store, "panics", "demo")
            .into_diagnostic()?
            .is_empty()
    );
    Ok(())
}

#[test]
fn edit_corrects_context_in_place_keeping_row_order() -> miette::Result<()> {
    cordial::init_tracing();
    let (_root, store) = demo_store()?;
    seed_two_jiff_rows(&store)?;

    let selector =
        ExceptionSelector::default().with_context("ext::jiff::fmt_strtime_broken_down_time");
    let update = ExceptionUpdate::default()
        .with_context("jiff::fmt_strtime_broken_down_time")
        .with_file("src/jiff/fmt.rs");
    let outcome =
        update_exception(&store, "visibility", "demo", &selector, &update).into_diagnostic()?;
    assert_eq!(
        outcome.before().context().as_deref(),
        Some("ext::jiff::fmt_strtime_broken_down_time")
    );

    let rows = read_rows(&store, "visibility")?;
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].context().as_deref(), Some("ext::jiff"));
    assert_eq!(
        rows[1].context().as_deref(),
        Some("jiff::fmt_strtime_broken_down_time")
    );
    assert_eq!(rows[1].file(), "src/jiff/fmt.rs");
    assert_eq!(rows[1].rule_id().as_deref(), Some("VISIBILITY-A"));
    assert_eq!(rows[1].reason(), "jiff fmt");
    Ok(())
}

#[test]
fn edit_with_empty_context_clears_it_and_rejects_noop_and_duplicates() -> miette::Result<()> {
    cordial::init_tracing();
    let (_root, store) = demo_store()?;
    seed_two_jiff_rows(&store)?;
    let selector = ExceptionSelector::default().with_context("ext::jiff");

    let err = update_exception(
        &store,
        "visibility",
        "demo",
        &selector,
        &ExceptionUpdate::default(),
    )
    .expect_err("update sets nothing");
    assert!(err.to_string().contains("at least one new value"), "{err}");

    // Make row 0 match row 1 apart from the file, then try to move its file onto row 1's.
    update_exception(
        &store,
        "visibility",
        "demo",
        &selector,
        &ExceptionUpdate::default()
            .with_context("ext::jiff::fmt_strtime_broken_down_time")
            .with_reason("jiff fmt"),
    )
    .into_diagnostic()?;
    let dup = update_exception(
        &store,
        "visibility",
        "demo",
        &ExceptionSelector::default().with_file("src/ext/jiff/mod.rs"),
        &ExceptionUpdate::default().with_file("src/ext/jiff/fmt.rs"),
    )
    .expect_err("would duplicate row 1");
    assert!(dup.to_string().contains("duplicates"), "{dup}");

    update_exception(
        &store,
        "visibility",
        "demo",
        &ExceptionSelector::default().with_file("src/ext/jiff/fmt.rs"),
        &ExceptionUpdate::default().with_context(""),
    )
    .into_diagnostic()?;
    let rows = read_rows(&store, "visibility")?;
    assert_eq!(rows[1].context(), &None);
    assert_eq!(
        rows[0].context().as_deref(),
        Some("ext::jiff::fmt_strtime_broken_down_time")
    );
    Ok(())
}

#[test]
fn stale_exceptions_reports_rows_that_match_no_finding() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src")).into_diagnostic()?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        include_str!("fixtures/panics/exceptions_patch.rs"),
    )
    .into_diagnostic()?;
    let store_root = tempfile::tempdir().into_diagnostic()?;
    let slug = cordial::project_slug_from_path(fixture.path());
    let store = StoreLayout::from_root(store_root.path(), slug.as_str());
    add_exception(
        &store,
        "panics",
        &slug,
        ExceptionEntry::new("src/lib.rs", "live").with_rule_id("PANIC-SOURCE-PANIC"),
    )
    .into_diagnostic()?;
    add_exception(
        &store,
        "panics",
        &slug,
        ExceptionEntry::new("src/lib.rs", "renamed away").with_context("ext::gone"),
    )
    .into_diagnostic()?;

    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store_root.path())
        .register(&PANICS_ETIQUETTE)
        .build();
    let outcome = session.run(&RunAll).into_diagnostic()?;
    let findings: Vec<_> = outcome.findings().collect();

    let mut sets = HashMap::new();
    sets.insert(
        "panics".to_string(),
        load_exceptions(&store, "panics", &slug).into_diagnostic()?,
    );
    let stale = stale_exceptions(&findings, &sets);
    assert_eq!(stale.len(), 1);
    assert_eq!(stale[0].etiquette(), "panics");
    assert_eq!(stale[0].entry().reason(), "renamed away");
    Ok(())
}

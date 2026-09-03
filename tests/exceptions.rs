use miette::{IntoDiagnostic, WrapErr};
use std::fs;
use std::path::PathBuf;

use cordial::{
    CoverageSkipEntry, ExceptionEntry, PANICS_ETIQUETTE, RunAll, Session, SessionBuilder,
    StoreLayout, add_coverage_skip, add_exception, backup_exception_files, coverage_skip_file_path,
    exception_file_path, load_exception_files, resolve_exceptions_root,
};

#[test]
fn exception_patch_suppresses_matching_panic_finding() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        include_str!("fixtures/panics/exceptions_patch.rs"),
    )
    .into_diagnostic()
    .wrap_err("write fixture")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let slug = cordial::project_slug_from_path(fixture.path());
    let exceptions_dir = store.path().join("exceptions").join("panics");
    fs::create_dir_all(&exceptions_dir)
        .into_diagnostic()
        .wrap_err("exceptions dir")?;
    fs::write(
        exceptions_dir.join(format!("{slug}.json")),
        r#"[
  {
    "file": "src/lib.rs",
    "rule_id": "PANIC-SOURCE-PANIC",
    "reason": "Demo panic is intentional"
  }
]"#,
    )
    .into_diagnostic()
    .wrap_err("write exception patch")?;

    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&PANICS_ETIQUETTE)
        .build();

    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    let findings: Vec<_> = outcome.findings().collect();
    assert_eq!(findings.len(), 2);

    let open = findings
        .iter()
        .filter(|f| f.disposition() == cordial::Disposition::Open)
        .count();
    let suppressed = findings
        .iter()
        .filter(|f| f.disposition() == cordial::Disposition::Suppressed)
        .count();
    assert_eq!(open, 1);
    assert_eq!(suppressed, 1);

    let checklist = fs::read_to_string(store.path().join("findings/panics.checklist.md"))
        .into_diagnostic()
        .wrap_err("checklist")?;
    assert!(checklist.contains("**Open items:** 1"));
    assert!(checklist.contains("Documented exceptions"));
    Ok(())
}

#[test]
fn quality_patches_alias_suppresses_matching_finding() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        include_str!("fixtures/panics/exceptions_alias.rs"),
    )
    .into_diagnostic()
    .wrap_err("write fixture")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let slug = cordial::project_slug_from_path(fixture.path());
    let patches_dir = store.path().join("quality").join("patches").join("panics");
    fs::create_dir_all(&patches_dir)
        .into_diagnostic()
        .wrap_err("patches dir")?;
    fs::write(
        patches_dir.join(format!("{slug}.json")),
        r#"[
  {
    "file": "src/lib.rs",
    "rule_id": "PANIC-SOURCE-PANIC",
    "reason": "Demo panic is intentional"
  }
]"#,
    )
    .into_diagnostic()
    .wrap_err("write quality patch")?;

    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&PANICS_ETIQUETTE)
        .build();

    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    let findings: Vec<_> = outcome.findings().collect();
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].disposition(), cordial::Disposition::Suppressed);
    Ok(())
}

#[test]
fn backup_exception_files_writes_slug_scoped_registry_tree() -> miette::Result<()> {
    cordial::init_tracing();
    let store_root = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let backup_root = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("backup tempdir")?;
    let store = StoreLayout::from_root(store_root.path(), "demo");
    store
        .ensure_dirs()
        .into_diagnostic()
        .wrap_err("ensure dirs")?;

    fs::create_dir_all(store.exceptions_dir().join("panics"))
        .into_diagnostic()
        .wrap_err("exceptions panics dir")?;
    fs::write(
        store.exceptions_dir().join("panics/demo.json"),
        r#"[{"file":"src/lib.rs","reason":"canonical"}]"#,
    )
    .into_diagnostic()
    .wrap_err("canonical exception")?;
    fs::create_dir_all(store.quality_patches_dir().join("panics"))
        .into_diagnostic()
        .wrap_err("quality panics dir")?;
    fs::write(
        store.quality_patches_dir().join("panics/demo.json"),
        r#"[{"file":"src/lib.rs","reason":"quality"}]"#,
    )
    .into_diagnostic()
    .wrap_err("quality patch")?;
    fs::create_dir_all(store.patches_dir())
        .into_diagnostic()
        .wrap_err("coverage patches dir")?;
    fs::write(
        store.patches_dir().join("chrono.json"),
        r#"[{"path":"chrono::DateTime","reason":"coverage"}]"#,
    )
    .into_diagnostic()
    .wrap_err("coverage patch")?;

    let copied = backup_exception_files(&store, backup_root.path())
        .into_diagnostic()
        .wrap_err("backup exceptions")?;
    assert_eq!(copied, 3);
    assert!(
        backup_root
            .path()
            .join("demo/exceptions/panics/demo.json")
            .is_file()
    );
    assert!(
        backup_root
            .path()
            .join("demo/quality/patches/panics/demo.json")
            .is_file()
    );
    assert!(
        backup_root
            .path()
            .join("demo/patches/chrono.json")
            .is_file()
    );
    Ok(())
}

#[test]
fn load_exception_files_restores_slug_scoped_registry_tree() -> miette::Result<()> {
    cordial::init_tracing();
    let store_root = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let backup_root = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("backup tempdir")?;
    fs::create_dir_all(backup_root.path().join("demo/exceptions/panics"))
        .into_diagnostic()
        .wrap_err("backup exceptions dir")?;
    fs::create_dir_all(backup_root.path().join("demo/quality/patches/panics"))
        .into_diagnostic()
        .wrap_err("backup quality dir")?;
    fs::create_dir_all(backup_root.path().join("demo/patches"))
        .into_diagnostic()
        .wrap_err("backup coverage dir")?;
    fs::write(
        backup_root.path().join("demo/exceptions/panics/demo.json"),
        r#"[{"file":"src/lib.rs","reason":"canonical backup"}]"#,
    )
    .into_diagnostic()
    .wrap_err("backup canonical")?;
    fs::write(
        backup_root
            .path()
            .join("demo/quality/patches/panics/demo.json"),
        r#"[{"file":"src/lib.rs","reason":"quality backup"}]"#,
    )
    .into_diagnostic()
    .wrap_err("backup quality")?;
    fs::write(
        backup_root.path().join("demo/patches/chrono.json"),
        r#"[{"path":"chrono::DateTime","reason":"coverage backup"}]"#,
    )
    .into_diagnostic()
    .wrap_err("backup coverage")?;

    let store = StoreLayout::from_root(store_root.path(), "demo");
    store
        .ensure_dirs()
        .into_diagnostic()
        .wrap_err("ensure dirs")?;
    fs::create_dir_all(store.exceptions_dir().join("panics"))
        .into_diagnostic()
        .wrap_err("stale exceptions dir")?;
    fs::write(
        store.exceptions_dir().join("panics/stale.json"),
        r#"[{"file":"src/stale.rs","reason":"stale"}]"#,
    )
    .into_diagnostic()
    .wrap_err("stale canonical")?;
    fs::create_dir_all(store.quality_patches_dir().join("panics"))
        .into_diagnostic()
        .wrap_err("stale quality dir")?;
    fs::write(
        store.quality_patches_dir().join("panics/stale.json"),
        r#"[{"file":"src/stale.rs","reason":"stale"}]"#,
    )
    .into_diagnostic()
    .wrap_err("stale quality")?;
    fs::create_dir_all(store.patches_dir())
        .into_diagnostic()
        .wrap_err("stale coverage dir")?;
    fs::write(
        store.patches_dir().join("stale.json"),
        r#"[{"path":"demo::Stale","reason":"stale"}]"#,
    )
    .into_diagnostic()
    .wrap_err("stale coverage")?;

    let copied = load_exception_files(&store, backup_root.path())
        .into_diagnostic()
        .wrap_err("load exceptions")?;
    assert_eq!(copied, 3);
    assert!(
        !store.exceptions_dir().join("panics/stale.json").exists(),
        "expected stale canonical exception to be replaced"
    );
    assert!(
        !store
            .quality_patches_dir()
            .join("panics/stale.json")
            .exists(),
        "expected stale quality patch to be replaced"
    );
    assert!(
        !store.patches_dir().join("stale.json").exists(),
        "expected stale coverage patch to be replaced"
    );
    assert_eq!(
        fs::read_to_string(store.exceptions_dir().join("panics/demo.json"))
            .into_diagnostic()
            .wrap_err("restored canonical")?,
        r#"[{"file":"src/lib.rs","reason":"canonical backup"}]"#
    );
    assert_eq!(
        fs::read_to_string(store.quality_patches_dir().join("panics/demo.json"))
            .into_diagnostic()
            .wrap_err("restored quality")?,
        r#"[{"file":"src/lib.rs","reason":"quality backup"}]"#
    );
    assert_eq!(
        fs::read_to_string(store.patches_dir().join("chrono.json"))
            .into_diagnostic()
            .wrap_err("restored coverage")?,
        r#"[{"path":"chrono::DateTime","reason":"coverage backup"}]"#
    );
    Ok(())
}

#[test]
fn load_exception_files_accepts_elicit_doc_registry_layout() -> miette::Result<()> {
    cordial::init_tracing();
    let store_root = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let backup_root = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("backup tempdir")?;
    fs::create_dir_all(backup_root.path().join("elicitation/patches"))
        .into_diagnostic()
        .wrap_err("elicit_doc coverage dir")?;
    fs::create_dir_all(
        backup_root
            .path()
            .join("elicitation/quality/patches/panics"),
    )
    .into_diagnostic()
    .wrap_err("elicit_doc quality dir")?;
    fs::write(
        backup_root.path().join("elicitation/patches/chrono.json"),
        r#"[{"path":"chrono::DateTime","reason":"upstream skip"}]"#,
    )
    .into_diagnostic()
    .wrap_err("elicit_doc coverage patch")?;
    fs::write(
        backup_root
            .path()
            .join("elicitation/quality/patches/panics/elicitation.json"),
        r#"[{"file":"src/lib.rs","reason":"upstream panic"}]"#,
    )
    .into_diagnostic()
    .wrap_err("elicit_doc quality patch")?;

    let store = StoreLayout::from_root(store_root.path(), "elicitation");
    let copied = load_exception_files(&store, backup_root.path())
        .into_diagnostic()
        .wrap_err("load elicit_doc layout")?;
    assert_eq!(copied, 2);
    assert!(store.patches_dir().join("chrono.json").is_file());
    assert!(
        store
            .quality_patches_dir()
            .join("panics/elicitation.json")
            .is_file()
    );
    assert!(!store.exceptions_dir().join("panics").is_dir());
    Ok(())
}

#[test]
fn load_exception_files_errors_when_slug_tree_is_missing() -> miette::Result<()> {
    cordial::init_tracing();
    let store_root = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let backup_root = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("backup tempdir")?;
    let store = StoreLayout::from_root(store_root.path(), "demo");
    let err = load_exception_files(&store, backup_root.path())
        .expect_err("missing slug tree should fail");
    assert!(err.to_string().contains("not found"));
    Ok(())
}

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

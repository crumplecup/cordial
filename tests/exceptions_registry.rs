//! The slug-scoped exception registry on disk: backing it up, loading it, and its layouts.

use miette::{IntoDiagnostic, WrapErr};
use std::fs;

use cordial::{StoreLayout, backup_exception_files, load_exception_files};

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

use miette::{IntoDiagnostic, WrapErr};
use std::fs;

use cordial::{RunAll, Session, SessionBuilder, TRACING_ETIQUETTE};

#[test]
fn tracing_etiquette_detects_missing_instrument() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        r#"
#[tracing::instrument]
pub fn instrumented() {
    let _ = 0;
}

pub fn traced() {
    let _ = 1;
}

pub fn untraced() {
    let _ = 2;
}

fn private_fn() {
    let _ = 3;
}
"#,
    )
    .into_diagnostic()
    .wrap_err("write fixture")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&TRACING_ETIQUETTE)
        .build();

    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    let open: Vec<_> = outcome
        .findings()
        .filter(|finding| finding.disposition() == cordial::Disposition::Open)
        .collect();
    assert_eq!(
        open.len(),
        2,
        "traced + untraced are pub without instrument"
    );

    let findings_dir = store.path().join("findings");
    let csv = fs::read_to_string(findings_dir.join("tracing-instrument.csv"))
        .into_diagnostic()
        .wrap_err("csv")?;
    assert!(csv.contains("untraced"));
    assert!(csv.contains("traced"));

    let checklist = fs::read_to_string(findings_dir.join("tracing-instrument.checklist.md"))
        .into_diagnostic()
        .wrap_err("checklist")?;
    assert!(checklist.contains("**Open gaps:** 2"));
    assert!(checklist.contains("untraced"));

    let summary = fs::read_to_string(findings_dir.join("tracing-summary.md"))
        .into_diagnostic()
        .wrap_err("summary")?;
    assert!(summary.contains("**2** open gaps"));
    Ok(())
}

#[test]
fn attribute_nodes_materialized_for_instrument() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        r#"#[tracing::instrument]
pub fn ok() {}
"#,
    )
    .into_diagnostic()
    .wrap_err("write fixture")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&TRACING_ETIQUETTE)
        .build();
    session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;

    let slug = cordial::project_slug_from_path(fixture.path());
    let cache = fs::read_to_string(store.path().join("cache").join(format!("{slug}.ir.json")))
        .into_diagnostic()
        .wrap_err("ir cache")?;
    assert!(cache.contains("Attribute"));
    assert!(cache.contains("HasAttr"));
    assert!(cache.contains("tracing::instrument"));
    Ok(())
}

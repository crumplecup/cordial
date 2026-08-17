use miette::{IntoDiagnostic, WrapErr};
use std::fs;

use cordial::{NamedRunFilter, PANICS_ETIQUETTE, Session, SessionBuilder, TRACING_ETIQUETTE};

#[test]
fn quality_cli_filter_runs_both_etiquettes() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        r#"
pub fn noisy() {
    panic!("boom");
}

pub fn quiet() {}
"#,
    )
    .into_diagnostic()
    .wrap_err("write fixture")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&PANICS_ETIQUETTE)
        .register(&TRACING_ETIQUETTE)
        .build();

    let filter = NamedRunFilter::etiquettes(&["panics", "tracing"]);
    let outcome = session
        .run(&filter)
        .into_diagnostic()
        .wrap_err("session run")?;
    let categories: Vec<_> = outcome
        .findings()
        .map(|finding| finding.rule().category().to_string())
        .collect();
    assert!(categories.iter().any(|category| category == "panics"));
    assert!(categories.iter().any(|category| category == "tracing"));
    assert!(store.path().join("findings/rollup-summary.md").is_file());
    Ok(())
}

#[test]
fn rollup_summary_lists_open_findings_by_etiquette() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        "pub fn boom() { panic!(\"x\"); }",
    )
    .into_diagnostic()
    .wrap_err("write fixture")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&PANICS_ETIQUETTE)
        .build();

    session
        .run(&NamedRunFilter::etiquettes(&["panics"]))
        .into_diagnostic()
        .wrap_err("session run")?;

    let summary = fs::read_to_string(store.path().join("findings/rollup-summary.md"))
        .into_diagnostic()
        .wrap_err("rollup summary")?;
    assert!(summary.contains("# Cordial rollup summary"));
    assert!(summary.contains("panics"));
    assert!(summary.contains("Open findings"));
    Ok(())
}

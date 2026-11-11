//! An exception row that suppresses nothing becomes an open worklist item.

use miette::{IntoDiagnostic, WrapErr};
use std::fs;

use cordial::{
    Disposition, PANICS_ETIQUETTE, RunAll, STALE_EXCEPTIONS_ETIQUETTE, Session, SessionBuilder,
};

fn run_with_rows(rows: &str) -> miette::Result<(Vec<(String, Disposition)>, String)> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src")).into_diagnostic()?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        include_str!("fixtures/panics/exceptions_patch.rs"),
    )
    .into_diagnostic()?;

    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let dir = store.path().join("exceptions").join("panics");
    fs::create_dir_all(&dir).into_diagnostic()?;
    fs::write(dir.join("fixture.json"), rows).into_diagnostic()?;

    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&*PANICS_ETIQUETTE)
        .register(&*STALE_EXCEPTIONS_ETIQUETTE)
        .build();
    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    let findings = outcome
        .findings()
        .map(|f| (f.rule().id().to_string(), f.disposition()))
        .collect();
    let checklist = fs::read_to_string(store.path().join("findings/stale-exceptions.checklist.md"))
        .unwrap_or_default();
    Ok((findings, checklist))
}

#[test]
fn live_exception_raises_no_stale_finding() -> miette::Result<()> {
    let (findings, _) = run_with_rows(
        r#"[{"file":"src/lib.rs","rule_id":"PANIC-SOURCE-PANIC","reason":"intentional"}]"#,
    )?;
    assert!(
        findings
            .iter()
            .all(|(rule, _)| rule != "EXCEPTION-STALE-001")
    );
    Ok(())
}

#[test]
fn dead_exception_raises_open_stale_finding() -> miette::Result<()> {
    let (findings, checklist) = run_with_rows(
        r#"[{"file":"src/lib.rs","line":999,"rule_id":"PANIC-SOURCE-PANIC","reason":"moved"}]"#,
    )?;
    let stale: Vec<_> = findings
        .iter()
        .filter(|(rule, _)| rule == "EXCEPTION-STALE-001")
        .collect();
    assert_eq!(stale.len(), 1);
    assert_eq!(stale[0].1, Disposition::Open);
    assert!(checklist.contains("**Open items:** 1"));
    assert!(checklist.contains("src/lib.rs:999"));
    Ok(())
}

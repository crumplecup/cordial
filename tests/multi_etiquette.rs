use miette::{IntoDiagnostic, WrapErr};
use std::fs;

use cordial::{PANICS_ETIQUETTE, RunAll, Session, SessionBuilder, TRACING_ETIQUETTE};

#[test]
fn multiple_etiquettes_share_loader_and_emit_both_reports() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        include_str!("fixtures/panics/multi_etiquette.rs"),
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

    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    let findings: Vec<_> = outcome.findings().collect();
    assert!(
        findings.iter().any(|f| f.rule().category() == "panics"),
        "expected panic finding"
    );
    assert!(
        findings.iter().any(|f| f.rule().category() == "tracing"),
        "expected tracing finding"
    );

    let findings_dir = store.path().join("findings");
    assert!(findings_dir.join("panics.csv").is_file());
    assert!(findings_dir.join("tracing-instrument.csv").is_file());
    assert!(findings_dir.join("rollup-summary.md").is_file());
    assert!(findings_dir.join("quality-report.md").is_file());
    assert!(findings_dir.join("summary.md").is_file());

    let slug = cordial::project_slug_from_path(fixture.path());
    let cache_path = store.path().join("cache").join(format!("{slug}.ir.json"));
    assert!(cache_path.is_file(), "single shared IR cache");
    Ok(())
}

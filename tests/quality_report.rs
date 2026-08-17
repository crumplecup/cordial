use cordial::{RunAll, Session, SessionBuilder, build_quality_report, quality_etiquettes};
use miette::{IntoDiagnostic, WrapErr};

#[test]
fn quality_report_lists_resolution_order() -> miette::Result<()> {
    let report = build_quality_report(&[]).into_diagnostic()?;
    assert_eq!(report.areas.len(), 5);
    assert_eq!(report.areas[0].title, "Error handling");
    assert_eq!(report.areas[3].title, "Tracing instrumentation");
    assert_eq!(report.areas[4].title, "Modularity");

    let body = cordial::render_quality_report_markdown(&report).into_diagnostic()?;
    assert!(body.contains("## Resolution order"));
    assert!(body.contains("foreign-error-attenuation.checklist.md"));
    assert!(body.contains("tracing-summary.md"));
    assert!(body.contains("modularity-summary.md"));

    let summary = cordial::render_quality_workspace_summary_markdown(&report).into_diagnostic()?;
    assert!(summary.contains("# Quality workspace summary"));
    assert!(summary.contains("## Heuristics"));
    assert!(summary.contains("quality-report.md"));
    assert!(summary.contains("Tracing instrumentation"));
    Ok(())
}

#[test]
fn quality_session_writes_quality_report_and_summary() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    std::fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    std::fs::write(
        fixture.path().join("Cargo.toml"),
        r#"[workspace]
members = ["."]

[workspace.package]
version = "0.1.0"
edition = "2024"

[package]
name = "quality_fixture"
version = { workspace = true }
edition = { workspace = true }
"#,
    )
    .into_diagnostic()
    .wrap_err("write manifest")?;
    std::fs::write(
        fixture.path().join("src/lib.rs"),
        "pub fn boom() { panic!(\"x\"); }\n",
    )
    .into_diagnostic()
    .wrap_err("write fixture")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let mut builder = SessionBuilder::new(fixture.path()).with_store_root(store.path());
    for etiquette in quality_etiquettes() {
        builder = builder.register(etiquette);
    }
    let session = builder.build();
    session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;

    let findings_dir = store.path().join("findings");
    let report = std::fs::read_to_string(findings_dir.join("quality-report.md"))
        .into_diagnostic()
        .wrap_err("quality-report.md")?;
    assert!(report.contains("# Code quality report"));
    assert!(report.contains("abort-site action items"));

    let summary = std::fs::read_to_string(findings_dir.join("summary.md"))
        .into_diagnostic()
        .wrap_err("summary.md")?;
    assert!(summary.contains("# Quality workspace summary"));
    assert!(findings_dir.join("rollup-summary.md").is_file());
    Ok(())
}

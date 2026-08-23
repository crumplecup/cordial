use cordial::{RunAll, Session, SessionBuilder, build_quality_report, quality_etiquettes};
use miette::{IntoDiagnostic, WrapErr};

#[test]
fn quality_report_lists_resolution_order() -> miette::Result<()> {
    let report = build_quality_report(&[]).into_diagnostic()?;
    assert_eq!(report.areas.len(), 12);
    assert_eq!(report.areas[0].title, "Error handling");
    assert_eq!(report.areas[1].title, "Antipatterns");
    assert_eq!(report.areas[2].title, "Derive patterns");
    assert_eq!(report.areas[4].title, "Tracing instrumentation");
    assert_eq!(report.areas[5].title, "Modularity");
    assert_eq!(report.areas[6].title, "Module visibility");
    assert_eq!(report.areas[7].title, "Cfg scatter");
    assert_eq!(report.areas[8].title, "CLI layout");
    assert_eq!(report.areas[9].title, "Glob imports");
    assert_eq!(report.areas[10].title, "Inline tests");
    assert_eq!(report.areas[11].title, "Verus compiler warnings");

    let body = cordial::render_quality_report_markdown(&report).into_diagnostic()?;
    assert!(body.contains("## Resolution order"));
    assert!(body.contains("foreign-error-attenuation.checklist.md"));
    assert!(body.contains("antipatterns.checklist.md"));
    assert!(body.contains("version-in-member.checklist.md"));
    assert!(body.contains("tracing-summary.md"));
    assert!(body.contains("modularity-summary.md"));
    assert!(body.contains("visibility.checklist.md"));
    assert!(body.contains("cfg-scatter-summary.md"));
    assert!(body.contains("cli-layout.checklist.md"));

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
    assert!(report.contains("| 2 | Antipatterns |"));
    assert!(report.contains("open gaps (other **1**)"));

    let summary = std::fs::read_to_string(findings_dir.join("summary.md"))
        .into_diagnostic()
        .wrap_err("summary.md")?;
    assert!(summary.contains("# Quality workspace summary"));
    assert!(findings_dir.join("rollup-summary.md").is_file());
    Ok(())
}

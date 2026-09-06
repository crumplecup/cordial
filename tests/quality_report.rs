use cordial::{RunAll, Session, SessionBuilder, build_quality_report, quality_etiquettes};
use miette::{IntoDiagnostic, WrapErr};

#[test]
fn quality_report_lists_resolution_order() -> miette::Result<()> {
    cordial::init_tracing();
    let report = build_quality_report(&[]).into_diagnostic()?;
    let area_titles = report
        .areas()
        .iter()
        .map(|area| area.title())
        .collect::<Vec<_>>();
    assert_eq!(
        area_titles,
        vec![
            "Error handling",
            "Tracing instrumentation",
            "Allow attributes",
            "Modularity",
            "Derive patterns",
            "Foreign error types",
            "Antipatterns",
            "Cfg scatter",
            "Cfg hygiene",
            "Module visibility",
            "CLI layout",
            "Crate attributes",
            "rustdoc warnings",
            "Glob imports",
            "Inline tests",
            "Verus compiler warnings",
            "Creusot diagnostics",
            "Dependency freshness",
            "Proof patterns",
            "Pageantry",
        ]
    );

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
    assert!(body.contains("crate-attrs.checklist.md"));
    assert!(body.contains("doc-warnings.checklist.md"));
    assert!(body.contains("creusot-diagnostics.checklist.md"));
    assert!(body.contains("dependency-freshness.checklist.md"));

    let summary = cordial::render_quality_workspace_summary_markdown(&report).into_diagnostic()?;
    assert!(summary.contains("# Quality workspace summary"));
    assert!(summary.contains("## Heuristics"));
    assert!(summary.contains("quality-report.md"));
    assert!(summary.contains("Tracing instrumentation"));
    Ok(())
}

#[test]
fn quality_session_writes_quality_report_and_summary() -> miette::Result<()> {
    cordial::init_tracing();
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
        include_str!("fixtures/panics/boom_x.rs"),
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
    assert!(report.contains("| 7 | Antipatterns |"));
    assert!(report.contains("open gaps (other **1**)"));

    let summary = std::fs::read_to_string(findings_dir.join("summary.md"))
        .into_diagnostic()
        .wrap_err("summary.md")?;
    assert!(summary.contains("# Quality workspace summary"));
    assert!(findings_dir.join("rollup-summary.md").is_file());
    Ok(())
}

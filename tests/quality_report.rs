use cordial::{
    ChecklistSource, CordialConfig, FindingSource, QualityReport, RunAll, Session, SessionBuilder,
    StoreLayout, Worklist, build_quality_report, quality_etiquettes,
};
use miette::{IntoDiagnostic, WrapErr};
use std::path::Path;

fn assert_area_checklist_counts(report: &QualityReport, findings_dir: &Path) -> miette::Result<()> {
    for area in report.areas() {
        let mut checklist_items = 0;
        let mut by_checklist = Vec::new();
        for checklist in area.checklists() {
            let before = checklist_items;
            let body = match std::fs::read_to_string(findings_dir.join(checklist.name())) {
                Ok(body) => body,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
                Err(error) => {
                    return Err(error)
                        .into_diagnostic()
                        .wrap_err_with(|| format!("{}: {}", area.title(), checklist.name()));
                }
            };
            let mut rule = "";
            for line in body.lines() {
                if line.starts_with("## ") {
                    rule = "";
                } else if let Some(heading) = line.strip_prefix("### ") {
                    rule = heading.trim();
                } else if line.starts_with("- [ ] ") && checklist.includes_rule(rule) {
                    checklist_items += 1;
                }
            }
            by_checklist.push((checklist.name(), checklist_items - before));
        }
        assert_eq!(
            area.open_items(),
            checklist_items,
            "{} checklist counts differ: {:?}; detail: {}",
            area.title(),
            by_checklist,
            area.detail()
        );
    }
    Ok(())
}

fn assert_all_items_have_rule_ids(
    report: &QualityReport,
    findings: &[&dyn cordial::Finding],
    store_root: &Path,
) -> miette::Result<()> {
    let layout = StoreLayout::from_root(store_root, "quality_fixture");
    let source = FindingSource::from_findings(ChecklistSource::new(&layout), findings);
    let list =
        Worklist::build(report, &layout, &source, &CordialConfig::default()).into_diagnostic()?;
    let missing: Vec<_> = list
        .items()
        .iter()
        .filter(|item| item.rule_id().is_none())
        .map(|item| (item.checklist(), item.item_line()))
        .collect();
    assert!(missing.is_empty(), "missing finding rules: {missing:?}");
    Ok(())
}

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
            "Foreign error types",
            "Antipatterns",
            "CLI layout",
            "Allow attributes",
            "Crate attributes",
            "Derive patterns",
            "Tracing instrumentation",
            "Inline tests",
            "Modularity",
            "Cfg scatter",
            "Cfg hygiene",
            "Module visibility",
            "Glob imports",
            "Pageantry",
            "Dependency freshness",
            "feature warnings",
            "rustdoc warnings",
            "Creusot diagnostics",
            "Verus compiler warnings",
            "Proof patterns",
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
    assert!(body.contains("feature-warnings.checklist.md"));

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
        format!(
            "{}\npub fn boxed() -> Result<(), Box<dyn std::error::Error>> {{ Ok(()) }}\npub fn unused(_value: i32) {{}}\n",
            include_str!("fixtures/panics/boom_x.rs")
        ),
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
    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;

    let findings: Vec<_> = outcome.findings().collect();
    let quality_report = build_quality_report(&findings).into_diagnostic()?;
    let findings_dir = store.path().join("findings");
    assert_area_checklist_counts(&quality_report, &findings_dir)?;
    assert_all_items_have_rule_ids(&quality_report, &findings, store.path())?;
    assert_eq!(quality_report.areas()[0].open_items(), 2);
    assert_eq!(quality_report.areas()[2].open_items(), 1);

    let report = std::fs::read_to_string(findings_dir.join("quality-report.md"))
        .into_diagnostic()
        .wrap_err("quality-report.md")?;
    assert!(report.contains("# Code quality report"));
    assert!(report.contains("abort-site action items"));
    assert!(report.contains("| 3 | Antipatterns |"));
    assert!(report.contains("open gaps (other **3**)"));

    let summary = std::fs::read_to_string(findings_dir.join("summary.md"))
        .into_diagnostic()
        .wrap_err("summary.md")?;
    assert!(summary.contains("# Quality workspace summary"));
    assert!(findings_dir.join("rollup-summary.md").is_file());
    Ok(())
}

#[test]
fn error_chain_fixture_agrees_with_checklists() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    std::fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    std::fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = \"error_chain_fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .into_diagnostic()
    .wrap_err("write manifest")?;
    std::fs::write(
        fixture.path().join("src/lib.rs"),
        include_str!("fixtures/internal_error_chain/compliance.rs"),
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
    let outcome = builder
        .build()
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    let findings: Vec<_> = outcome.findings().collect();
    let report = build_quality_report(&findings).into_diagnostic()?;
    let findings_dir = store.path().join("findings");
    assert_area_checklist_counts(&report, &findings_dir)?;
    assert_all_items_have_rule_ids(&report, &findings, store.path())?;
    assert_eq!(report.areas()[0].open_items(), 5);
    let internal = std::fs::read_to_string(findings_dir.join("internal-error-chain.checklist.md"))
        .into_diagnostic()?;
    assert_eq!(
        internal
            .matches(" — also in foreign-error-attenuation.checklist.md")
            .count(),
        3
    );
    Ok(())
}

//! Shared policy inheritance and enforcement for built-ins and plugins.

#[path = "../examples/custom_plugins/quality.rs"]
mod quality;
#[path = "lint_configuration/workspace.rs"]
mod workspace;

use std::{fs, path::Path};

use cordial::{CordialConfig, LintLocation, RunAll, Session, SessionBuilder, TRACING_ETIQUETTE};
use miette::{IntoDiagnostic, WrapErr};

fn enabled(config: &CordialConfig, file: Option<&str>) -> bool {
    config.lint_enabled(
        "custom",
        "CUSTOM-001",
        LintLocation::new(
            Path::new("/workspace"),
            Some(Path::new("/workspace/crates/app")),
            "app",
            file.map(Path::new),
        ),
    )
}

#[test]
fn narrow_overrides_restore_rules_without_matching_neighbor_paths() -> miette::Result<()> {
    cordial::init_tracing();
    let config: CordialConfig = toml::from_str(
        r#"
[lints.custom]
enabled = false
[[lints.custom.paths]]
path = "crates/app/src"
base = "workspace"
enabled = true
[[lints.custom.paths]]
path = "src/generated"
enabled = false
[[lints.custom.paths]]
path = "src/generated/keep.rs"
rules = { CUSTOM-001 = true }
"#,
    )
    .into_diagnostic()?;
    assert!(config.etiquette_may_run("custom"));
    assert!(!enabled(&config, None));
    assert!(enabled(&config, Some("src/lib.rs")));
    assert!(!enabled(&config, Some("src/generated/code.rs")));
    assert!(enabled(&config, Some("src/generated/keep.rs")));
    assert!(enabled(&config, Some("src/generated_neighbor/code.rs")));
    assert!(enabled(
        &config,
        Some("/workspace/crates/app/src/generated/keep.rs")
    ));
    assert!(enabled(&config, Some(".\\src\\generated\\keep.rs")));
    Ok(())
}

#[test]
fn rule_switches_skip_paths_and_crate_filters_have_deterministic_precedence() -> miette::Result<()>
{
    cordial::init_tracing();
    let config: CordialConfig = toml::from_str(
        r#"
[lints.custom]
rules = { CUSTOM-001 = false }
skip_paths = { CUSTOM-001 = ["src/skipped"] }
[[lints.custom.paths]]
path = "src"
crate = "other"
enabled = true
[[lints.custom.paths]]
path = "src/skipped/keep.rs"
rules = { CUSTOM-001 = true }
[[lints.custom.paths]]
path = "src/skipped/keep.rs"
enabled = false
rules = { CUSTOM-001 = true }
"#,
    )
    .into_diagnostic()?;
    assert!(!enabled(&config, None));
    assert!(!enabled(&config, Some("src/lib.rs")));
    assert!(!enabled(&config, Some("src/skipped/code.rs")));
    assert!(enabled(&config, Some("src/skipped/keep.rs")));
    Ok(())
}

#[test]
fn legacy_antipattern_exclusions_remain_component_scoped() -> miette::Result<()> {
    cordial::init_tracing();
    let config: CordialConfig = toml::from_str(
        r#"
[antipatterns.skip_paths]
RULE-001 = ["src/generated"]
"#,
    )
    .into_diagnostic()?;
    for (file, expected) in [
        ("src/generated/a.rs", false),
        ("src/generated_neighbor/a.rs", true),
    ] {
        assert_eq!(
            config.lint_enabled(
                "antipatterns",
                "RULE-001",
                LintLocation::new(
                    Path::new("/workspace"),
                    Some(Path::new("/workspace")),
                    "app",
                    Some(Path::new(file)),
                )
            ),
            expected
        );
    }
    Ok(())
}

#[test]
fn plugin_policy_filters_findings_and_reports_without_a_builtin_config_entry() -> miette::Result<()>
{
    cordial::init_tracing();
    let temp = tempfile::tempdir().into_diagnostic()?;
    fs::create_dir_all(temp.path().join("src")).into_diagnostic()?;
    fs::write(
        temp.path().join("src/lib.rs"),
        "pub fn pending() { todo!() }\n",
    )
    .into_diagnostic()?;
    let store = tempfile::tempdir().into_diagnostic()?;
    let session = SessionBuilder::new(temp.path())
        .with_store_root(store.path())
        .with_store_home(store.path())
        .register_plugin(&*quality::ACME_STYLE)
        .build();
    let baseline = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("baseline plugin run")?;
    assert!(
        baseline
            .findings()
            .any(|finding| finding.rule().id() == "ACME-TODO-001")
    );
    let layout = cordial::StoreLayout::from_root(store.path(), "fixture".to_string());
    let ledger = cordial::GateLedger::read(&layout).into_diagnostic()?;
    assert_eq!(ledger.entries()["acme-todo"].open_count(), &1);
    fs::write(
        temp.path().join("cordial.toml"),
        r#"
[lints.acme-todo]
skip_paths = { ACME-TODO-001 = ["src/lib.rs"] }
"#,
    )
    .into_diagnostic()?;
    let filtered = session.run(&RunAll).into_diagnostic()?;
    assert!(
        !filtered
            .findings()
            .any(|finding| finding.rule().id() == "ACME-TODO-001")
    );
    let csv = fs::read_to_string(store.path().join("findings/acme-todo.csv")).into_diagnostic()?;
    assert!(!csv.contains("pending"));
    // A narrower enable must cause the disabled plugin to be scheduled again.
    fs::write(
        temp.path().join("cordial.toml"),
        r#"
[lints.acme-todo]
enabled = false
[[lints.acme-todo.paths]]
path = "src/lib.rs"
rules = { ACME-TODO-001 = true }
"#,
    )
    .into_diagnostic()?;
    let restored = session.run(&RunAll).into_diagnostic()?;
    assert!(
        restored
            .findings()
            .any(|finding| finding.rule().id() == "ACME-TODO-001")
    );
    Ok(())
}

#[test]
fn tracing_file_exclusion_reaches_assessment_and_report() -> miette::Result<()> {
    cordial::init_tracing();
    let temp = tempfile::tempdir().into_diagnostic()?;
    fs::create_dir_all(temp.path().join("src")).into_diagnostic()?;
    fs::write(temp.path().join("src/lib.rs"), "pub fn pending() {}\n").into_diagnostic()?;
    fs::write(
        temp.path().join("cordial.toml"),
        r#"
[lints.tracing]
skip_paths = { TRACING-MISSING-INSTRUMENT = ["src/lib.rs"] }
"#,
    )
    .into_diagnostic()?;
    let store = tempfile::tempdir().into_diagnostic()?;
    let session = SessionBuilder::new(temp.path())
        .with_store_root(store.path())
        .with_store_home(store.path())
        .register(&*TRACING_ETIQUETTE)
        .build();
    let result = session.run(&RunAll).into_diagnostic()?;
    assert!(
        !result
            .findings()
            .any(|finding| finding.rule().id() == "TRACING-MISSING-INSTRUMENT")
    );
    let report =
        fs::read_to_string(store.path().join("findings/quality-report.md")).into_diagnostic()?;
    assert!(!report.contains("pending"));
    Ok(())
}

//! Which config wins: workspace over store home over defaults, fallback on missing or unreadable files, and `enabled = false`.

use miette::{IntoDiagnostic, WrapErr};
use std::fs;

use cordial::{
    CordialConfig, VisibilityThresholds, load_cordial_config, load_visibility_thresholds,
};

#[test]
fn shared_plugin_policy_merges_store_and_workspace_tables() -> miette::Result<()> {
    cordial::init_tracing();
    let workspace = tempfile::tempdir().into_diagnostic()?;
    let store = tempfile::tempdir().into_diagnostic()?;
    fs::write(store.path().join("cordial.toml"),
        "[lints.custom]\nenabled = false\n[lints.custom.rules]\nCUSTOM-001 = false\nCUSTOM-002 = true\n")
        .into_diagnostic()?;
    fs::write(
        workspace.path().join("cordial.toml"),
        "[lints.custom.rules]\nCUSTOM-001 = true\n",
    )
    .into_diagnostic()?;
    let config = load_cordial_config(workspace.path(), store.path());
    let location =
        cordial::LintLocation::new(workspace.path(), Some(workspace.path()), "app", None);
    assert!(!config.etiquette_enabled("custom"));
    assert!(config.etiquette_may_run("custom"));
    assert!(config.lint_enabled("custom", "CUSTOM-001", location));
    assert!(config.lint_enabled("custom", "CUSTOM-002", location));
    assert!(!config.lint_enabled("custom", "CUSTOM-003", location));
    Ok(())
}

#[test]
fn missing_config_files_use_default() -> miette::Result<()> {
    cordial::init_tracing();
    let workspace = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("workspace")?;
    let store_home = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store home")?;
    let loaded = load_cordial_config(workspace.path(), store_home.path());
    assert_eq!(loaded, CordialConfig::default());
    Ok(())
}

#[test]
fn unreadable_config_falls_back_to_default() -> miette::Result<()> {
    cordial::init_tracing();
    let workspace = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("workspace")?;
    let store_home = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store home")?;
    fs::write(workspace.path().join("cordial.toml"), "this is not toml {")
        .into_diagnostic()
        .wrap_err("bad toml")?;
    let loaded = load_cordial_config(workspace.path(), store_home.path());
    assert_eq!(loaded, CordialConfig::default());
    Ok(())
}

#[test]
fn workspace_toml_overrides_store_home() -> miette::Result<()> {
    cordial::init_tracing();
    let workspace = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("workspace")?;
    let store_home = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store home")?;
    fs::write(
        store_home.path().join("cordial.toml"),
        r#"
[visibility]
max_crate_names_for_flat = 20
min_module_names = 7
"#,
    )
    .into_diagnostic()
    .wrap_err("home config")?;
    fs::write(
        workspace.path().join("cordial.toml"),
        r#"
[visibility]
min_module_names = 3
"#,
    )
    .into_diagnostic()
    .wrap_err("workspace config")?;

    let loaded = load_cordial_config(workspace.path(), store_home.path());
    assert_eq!(loaded.visibility().max_crate_names_for_flat(), 20);
    assert_eq!(loaded.visibility().min_module_names(), 3);
    assert!(loaded.visibility().prefer_root());
    Ok(())
}

#[test]
fn store_home_overrides_default() -> miette::Result<()> {
    cordial::init_tracing();
    let workspace = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("workspace")?;
    let store_home = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store home")?;
    fs::write(
        store_home.path().join("cordial.toml"),
        r#"
[visibility]
prefer_root = false
"#,
    )
    .into_diagnostic()
    .wrap_err("home config")?;

    let loaded = load_visibility_thresholds(workspace.path(), store_home.path());
    assert!(!loaded.prefer_root());
    assert_eq!(
        loaded.max_crate_names_for_flat(),
        VisibilityThresholds::default().max_crate_names_for_flat()
    );
    Ok(())
}

#[test]
fn enabled_false_turns_the_etiquette_off() -> miette::Result<()> {
    cordial::init_tracing();
    let workspace = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("workspace")?;
    let store_home = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store home")?;
    fs::write(
        workspace.path().join("cordial.toml"),
        r#"
[doc_warnings]
enabled = false

[creusot_diagnostics]
enabled = false

[dependency_freshness]
enabled = false

[panics]
enabled = false

[impl-coverage]
enabled = false
"#,
    )
    .into_diagnostic()
    .wrap_err("workspace config")?;

    let loaded = load_cordial_config(workspace.path(), store_home.path());
    assert!(!loaded.etiquette_enabled("doc_warnings"));
    assert!(!loaded.etiquette_enabled("creusot_diagnostics"));
    assert!(!loaded.etiquette_enabled("dependency_freshness"));
    assert!(!loaded.etiquette_enabled("panics"));
    assert!(!loaded.etiquette_enabled("impl-coverage"));
    assert!(loaded.etiquette_enabled("tracing"));
    assert!(
        loaded.etiquette_enabled("custom_plugin"),
        "unknown ids stay on"
    );
    Ok(())
}

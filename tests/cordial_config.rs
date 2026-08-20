use miette::{IntoDiagnostic, WrapErr};
use std::fs;

use cordial::{
    CordialConfig, VisibilityThresholds, load_cordial_config, load_visibility_thresholds,
};

#[test]
fn missing_config_files_use_default() -> miette::Result<()> {
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
fn tracing_toml_overrides_default() -> miette::Result<()> {
    let workspace = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("workspace")?;
    let store_home = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store home")?;
    fs::write(
        workspace.path().join("cordial.toml"),
        r#"
[tracing]
extra_skip = ["payload", "blob"]
"#,
    )
    .into_diagnostic()
    .wrap_err("workspace config")?;

    let loaded = load_cordial_config(workspace.path(), store_home.path());
    assert_eq!(
        loaded.tracing().extra_skip().as_slice(),
        ["payload".to_string(), "blob".to_string()]
    );
    Ok(())
}

#[test]
fn derives_toml_overrides_default() -> miette::Result<()> {
    let workspace = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("workspace")?;
    let store_home = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store home")?;
    fs::write(
        workspace.path().join("cordial.toml"),
        r#"
[derives]
max_constructor_args = 5
min_fluent_setters = 3
"#,
    )
    .into_diagnostic()
    .wrap_err("workspace config")?;

    let loaded = load_cordial_config(workspace.path(), store_home.path());
    assert_eq!(loaded.derives().max_constructor_args(), 5);
    assert_eq!(loaded.derives().min_fluent_setters(), 3);
    Ok(())
}

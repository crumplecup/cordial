use miette::{IntoDiagnostic, WrapErr};
use std::fs;

use cordial::{
    CordialConfig, VisibilityThresholds, load_cordial_config, load_visibility_thresholds,
};

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
fn tracing_toml_overrides_default() -> miette::Result<()> {
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
[tracing]
extra_skip = ["payload", "blob"]
apply_gate_crates = { fixture_kani = "kani" }
apply_skip_crates = ["fixture_verus"]
"#,
    )
    .into_diagnostic()
    .wrap_err("workspace config")?;

    let loaded = load_cordial_config(workspace.path(), store_home.path());
    assert_eq!(
        loaded.tracing().extra_skip().as_slice(),
        ["payload".to_string(), "blob".to_string()]
    );
    assert_eq!(
        loaded
            .tracing()
            .apply_gate_crates()
            .get("fixture_kani")
            .map(String::as_str),
        Some("kani")
    );
    assert_eq!(
        loaded.tracing().apply_skip_crates().as_slice(),
        ["fixture_verus".to_string()]
    );
    let subscriber = loaded.tracing().subscriber();
    assert!(subscriber.init_in_main());
    assert!(subscriber.init_in_tests());
    assert!(subscriber.helper_in_lib());
    assert!(subscriber.rust_log_fallback());
    assert!(subscriber.idempotent());
    let stdio = loaded.tracing().stdio();
    assert!(stdio.println());
    assert!(stdio.eprintln());
    assert!(stdio.print());
    assert!(stdio.eprint());
    assert!(stdio.dbg());
    assert!(stdio.skip_cargo_protocol());
    assert_eq!(
        stdio.skip_folders().as_slice(),
        ["tests/fixtures".to_string(), "tests/parity".to_string()]
    );
    Ok(())
}

#[test]
fn tracing_subscriber_toml_overrides_default() -> miette::Result<()> {
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
[tracing.subscriber]
init_in_main = false
init_in_tests = false
helper_in_lib = false
rust_log_fallback = false
idempotent = false
"#,
    )
    .into_diagnostic()
    .wrap_err("workspace config")?;

    let loaded = load_cordial_config(workspace.path(), store_home.path());
    let subscriber = loaded.tracing().subscriber();
    assert!(!subscriber.init_in_main());
    assert!(!subscriber.init_in_tests());
    assert!(!subscriber.helper_in_lib());
    assert!(!subscriber.rust_log_fallback());
    assert!(!subscriber.idempotent());
    Ok(())
}

#[test]
fn tracing_stdio_toml_overrides_default() -> miette::Result<()> {
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
[tracing.stdio]
dbg = false
println = false
skip_cargo_protocol = false
skip_folders = ["src/generated"]
"#,
    )
    .into_diagnostic()
    .wrap_err("workspace config")?;

    let loaded = load_cordial_config(workspace.path(), store_home.path());
    let stdio = loaded.tracing().stdio();
    assert!(!stdio.dbg());
    assert!(!stdio.println());
    assert!(stdio.eprintln());
    assert!(!stdio.skip_cargo_protocol());
    assert_eq!(
        stdio.skip_folders().as_slice(),
        ["src/generated".to_string()]
    );
    Ok(())
}

#[test]
fn derives_toml_overrides_default() -> miette::Result<()> {
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

#[test]
fn crate_attrs_toml_overrides_default() -> miette::Result<()> {
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
[crate_attrs]
forbid_unsafe = false
missing_docs = true
allow_unsafe = ["ffi"]
allow_missing_docs = ["legacy"]
"#,
    )
    .into_diagnostic()
    .wrap_err("workspace config")?;

    let loaded = load_cordial_config(workspace.path(), store_home.path());
    let crate_attrs = loaded.crate_attrs();
    assert!(!crate_attrs.forbid_unsafe());
    assert!(crate_attrs.missing_docs());
    assert_eq!(crate_attrs.allow_unsafe(), &["ffi".to_string()]);
    assert_eq!(crate_attrs.allow_missing_docs(), &["legacy".to_string()]);
    Ok(())
}

#[test]
fn doc_warnings_toml_overrides_default() -> miette::Result<()> {
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
document_private_items = true
all_features = true
skip_crates = ["proc_helper"]
"#,
    )
    .into_diagnostic()
    .wrap_err("workspace config")?;

    let loaded = load_cordial_config(workspace.path(), store_home.path());
    let doc_warnings = loaded.doc_warnings();
    assert!(doc_warnings.document_private_items());
    assert!(doc_warnings.all_features());
    assert_eq!(doc_warnings.skip_crates(), &["proc_helper".to_string()]);
    assert!(doc_warnings.enabled());
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
    assert!(!loaded.etiquette_enabled("panics"));
    assert!(!loaded.etiquette_enabled("impl-coverage"));
    assert!(loaded.etiquette_enabled("tracing"));
    assert!(
        loaded.etiquette_enabled("custom_plugin"),
        "unknown ids stay on"
    );
    Ok(())
}

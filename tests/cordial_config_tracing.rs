//! The `[tracing]`, `[tracing.subscriber]` and stdio tables override the defaults.

use miette::{IntoDiagnostic, WrapErr};
use std::fs;

use cordial::load_cordial_config;

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

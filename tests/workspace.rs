use miette::{IntoDiagnostic, WrapErr};
use std::fs;

use cordial::{PANICS_ETIQUETTE, RunAll, Session, SessionBuilder};

#[test]
fn workspace_run_analyzes_each_member() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    write_workspace(
        fixture.path(),
        r#"
[workspace]
members = ["crates/alpha", "crates/beta"]
resolver = "2"
"#,
    )?;
    write_member(
        fixture.path(),
        "crates/alpha",
        "alpha",
        include_str!("fixtures/panics/workspace_alpha.rs"),
    )?;
    write_member(
        fixture.path(),
        "crates/beta",
        "beta",
        include_str!("fixtures/panics/workspace_beta.rs"),
    )?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&PANICS_ETIQUETTE)
        .build();

    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    let findings: Vec<_> = outcome.findings().collect();
    assert_eq!(
        findings.len(),
        2,
        "expected one panic finding per member crate"
    );

    assert!(store.path().join("cache/alpha.ir.json").is_file());
    assert!(store.path().join("cache/beta.ir.json").is_file());
    assert!(store.path().join("cache/alpha.ir.digests.json").is_file());
    assert!(store.path().join("cache/beta.ir.digests.json").is_file());
    assert!(store.path().join("findings/rollup-summary.md").is_file());
    Ok(())
}

fn write_workspace(root: &std::path::Path, body: &str) -> miette::Result<()> {
    fs::write(root.join("Cargo.toml"), body)
        .into_diagnostic()
        .wrap_err("workspace manifest")?;
    Ok(())
}

fn write_member(
    root: &std::path::Path,
    rel: &str,
    name: &str,
    lib_body: &str,
) -> miette::Result<()> {
    let crate_root = root.join(rel);
    fs::create_dir_all(crate_root.join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        crate_root.join("Cargo.toml"),
        format!(
            r#"
[package]
name = "{name}"
version = "0.1.0"
edition = "2024"
"#
        ),
    )
    .into_diagnostic()
    .wrap_err("member manifest")?;
    fs::write(crate_root.join("src/lib.rs"), lib_body)
        .into_diagnostic()
        .wrap_err("lib rs")?;
    Ok(())
}

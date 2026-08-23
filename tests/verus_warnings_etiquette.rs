use std::fs;
use std::path::PathBuf;

use cordial::{
    RunAll, Session, SessionBuilder, VERUS_WARNINGS_ETIQUETTE, VerusWarningRuleId,
    crate_is_verus_target, parse_verus_compiler_output, scan_crate_verus_warnings,
};
use miette::{IntoDiagnostic, WrapErr};

const CANARY: &str = include_str!("fixtures/quality/verus_warnings/canary.stderr");

fn canary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/quality/verus_warnings/canary.stderr")
}

#[test]
fn parse_amenable_canary_keeps_two_unique_warnings() {
    let crate_root = PathBuf::from("/workspace");
    let records = parse_verus_compiler_output(CANARY, &crate_root);
    assert_eq!(records.len(), 2);
    assert!(
        records
            .iter()
            .all(|record| record.rule_id == VerusWarningRuleId::Warning001)
    );
    assert!(
        records
            .iter()
            .any(|record| record.line == 132 && record.snippet.contains("impl_tuple_evidence"))
    );
    assert!(
        records
            .iter()
            .any(|record| record.line == 446 && record.snippet.contains("autoderive Clone"))
    );
}

#[test]
fn parse_drops_summary_lines_and_errors() {
    let output = "\
error: expected `;`
 --> src/lib.rs:1:1

warning: 3 warnings emitted
";
    let records = parse_verus_compiler_output(output, &PathBuf::from("/workspace"));
    assert!(records.is_empty());
}

#[test]
fn crate_named_verus_is_a_target() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let crate_root = fixture.path().join("demo_verus");
    fs::create_dir_all(crate_root.join("src")).into_diagnostic()?;
    fs::write(
        crate_root.join("Cargo.toml"),
        "[package]\nname = \"demo_verus\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .into_diagnostic()?;
    fs::write(crate_root.join("src/lib.rs"), "pub fn ok() {}\n").into_diagnostic()?;
    assert!(crate_is_verus_target(&crate_root));
    Ok(())
}

#[test]
fn crate_with_vstd_dep_is_a_target() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = \"proofs\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nvstd = \"0.0\"\n",
    )
    .into_diagnostic()?;
    assert!(crate_is_verus_target(fixture.path()));
    Ok(())
}

#[test]
fn ordinary_crate_is_not_a_verus_target() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = \"plain\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .into_diagnostic()?;
    assert!(!crate_is_verus_target(fixture.path()));
    Ok(())
}

#[test]
fn scan_skips_non_verus_crate_without_invoking_compiler() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src")).into_diagnostic()?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = \"plain\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .into_diagnostic()?;
    fs::write(fixture.path().join("src/lib.rs"), "pub fn ok() {}\n").into_diagnostic()?;
    let records = scan_crate_verus_warnings(fixture.path())
        .into_diagnostic()
        .wrap_err("scan")?;
    assert!(records.is_empty());
    Ok(())
}

#[cfg(unix)]
#[test]
fn session_writes_checklist_from_injected_verus() -> miette::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let crate_root = fixture.path().join("canary_verus");
    fs::create_dir_all(crate_root.join("src")).into_diagnostic()?;
    fs::write(
        crate_root.join("Cargo.toml"),
        "[package]\nname = \"canary_verus\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .into_diagnostic()?;
    fs::write(crate_root.join("src/lib.rs"), "pub fn ok() {}\n").into_diagnostic()?;

    let fake = fixture.path().join("fake-verus");
    let script = format!("#!/bin/sh\ncat \"{}\"\n", canary_path().display());
    fs::write(&fake, script).into_diagnostic()?;
    let mut perms = fs::metadata(&fake).into_diagnostic()?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&fake, perms).into_diagnostic()?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;

    let previous = std::env::var_os("CORDIAL_VERUS");
    // SAFETY: test process owns this env var for the duration of the session run.
    unsafe {
        std::env::set_var("CORDIAL_VERUS", &fake);
    }
    let outcome = {
        let session = SessionBuilder::new(&crate_root)
            .with_store_root(store.path())
            .register(&VERUS_WARNINGS_ETIQUETTE)
            .build();
        session.run(&RunAll)
    };
    match previous {
        Some(value) => unsafe {
            std::env::set_var("CORDIAL_VERUS", value);
        },
        None => unsafe {
            std::env::remove_var("CORDIAL_VERUS");
        },
    }
    let outcome = outcome.into_diagnostic().wrap_err("session run")?;
    assert_eq!(outcome.findings().count(), 2);

    let findings_dir = store.path().join("findings");
    let csv = fs::read_to_string(findings_dir.join("verus-warnings.csv"))
        .into_diagnostic()
        .wrap_err("csv")?;
    assert!(csv.contains("VERUS-WARNING-001"));
    assert!(csv.contains("impl_tuple_evidence"));
    assert!(csv.contains("autoderive Clone"));

    let checklist = fs::read_to_string(findings_dir.join("verus-warnings.checklist.md"))
        .into_diagnostic()
        .wrap_err("checklist")?;
    assert!(checklist.contains("**Open items:** 2"));
    assert!(checklist.contains("deny-warnings"));

    let summary = fs::read_to_string(findings_dir.join("verus-warnings-summary.md"))
        .into_diagnostic()
        .wrap_err("summary")?;
    assert!(summary.contains("**2** Verus compiler warnings"));
    Ok(())
}

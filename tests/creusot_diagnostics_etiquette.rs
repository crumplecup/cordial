use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use cordial::{
    CREUSOT_DIAGNOSTICS_ETIQUETTE, CreusotDiagnosticRuleId, CreusotDiagnosticsThresholds, RunAll,
    Session, SessionBuilder, crate_is_creusot_target, load_cordial_config,
    parse_creusot_compiler_output, scan_crate_creusot_diagnostics,
};
use miette::{IntoDiagnostic, WrapErr};

const CANARY: &str = include_str!("fixtures/quality/creusot_diagnostics/canary.stderr");

static CREUSOT_ENV_LOCK: Mutex<()> = Mutex::new(());

fn canary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/quality/creusot_diagnostics/canary.stderr")
}

#[test]
fn parse_canary_keeps_warning_and_failure() {
    cordial::init_tracing();
    let crate_root = PathBuf::from("/workspace");
    let records = parse_creusot_compiler_output(CANARY, &crate_root, false).expect("parse canary");
    assert_eq!(records.len(), 2, "{records:?}");
    assert!(records.iter().any(|record| {
        record.rule_id() == CreusotDiagnosticRuleId::Warning001
            && record.line() == 12
            && record.snippet().contains("unused variable")
    }));
    assert!(records.iter().any(|record| {
        record.rule_id() == CreusotDiagnosticRuleId::Failure001
            && record.line() == 20
            && record.snippet().contains("postcondition")
    }));
}

#[test]
fn parse_failed_without_span_emits_crate_level_failure() {
    cordial::init_tracing();
    let crate_root = PathBuf::from("/workspace");
    let records = parse_creusot_compiler_output(
        "error: aborting due to previous error\n",
        &crate_root,
        false,
    )
    .expect("parse");
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0].rule_id(), CreusotDiagnosticRuleId::Failure001);
    assert_eq!(records[0].line(), 1);
    assert_eq!(records[0].file(), &PathBuf::from("/workspace/Cargo.toml"));
}

#[test]
fn parse_success_drops_summary_lines() {
    cordial::init_tracing();
    let records = parse_creusot_compiler_output(
        "warning: 3 warnings emitted\nerror: 1 error emitted\n",
        &PathBuf::from("/workspace"),
        true,
    )
    .expect("parse");
    assert!(records.is_empty(), "{records:?}");
}

#[test]
fn parse_workspace_relative_span_resolves_from_workspace_root() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let crate_root = fixture.path().join("crates/demo_creusot");
    fs::create_dir_all(crate_root.join("src")).into_diagnostic()?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/demo_creusot\"]\nresolver = \"3\"\n",
    )
    .into_diagnostic()?;
    fs::write(
        crate_root.join("Cargo.toml"),
        "[package]\nname = \"demo_creusot\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .into_diagnostic()?;
    fs::write(crate_root.join("src/lib.rs"), "pub fn ok() {}\n").into_diagnostic()?;

    let records = parse_creusot_compiler_output(
        "warning: unused variable\n  --> crates/demo_creusot/src/lib.rs:1:8\n",
        &crate_root,
        true,
    )
    .expect("parse");

    assert_eq!(records.len(), 1);
    assert_eq!(
        records[0].file(),
        &fixture.path().join("crates/demo_creusot/src/lib.rs")
    );
    Ok(())
}

#[test]
fn crate_named_creusot_is_a_target() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let crate_root = fixture.path().join("demo_creusot");
    fs::create_dir_all(crate_root.join("src")).into_diagnostic()?;
    fs::write(
        crate_root.join("Cargo.toml"),
        "[package]\nname = \"demo_creusot\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .into_diagnostic()?;
    fs::write(crate_root.join("src/lib.rs"), "pub fn ok() {}\n").into_diagnostic()?;
    assert!(crate_is_creusot_target(&crate_root));
    Ok(())
}

#[test]
fn package_named_creusot_is_a_target_even_when_directory_is_plain() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = \"proof_surface_creusot\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .into_diagnostic()?;
    assert!(crate_is_creusot_target(fixture.path()));
    Ok(())
}

#[test]
fn crate_with_creusot_std_dep_is_a_target() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = \"proofs\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\ncreusot-std = \"0.0\"\n",
    )
    .into_diagnostic()?;
    assert!(crate_is_creusot_target(fixture.path()));
    Ok(())
}

#[test]
fn ordinary_crate_is_not_a_creusot_target() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = \"plain\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .into_diagnostic()?;
    assert!(!crate_is_creusot_target(fixture.path()));
    Ok(())
}

#[test]
fn scan_skips_non_creusot_crate_without_invoking_runner() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src")).into_diagnostic()?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = \"plain\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .into_diagnostic()?;
    fs::write(fixture.path().join("src/lib.rs"), "pub fn ok() {}\n").into_diagnostic()?;
    let records = scan_crate_creusot_diagnostics(
        fixture.path(),
        "plain",
        &CreusotDiagnosticsThresholds::default(),
    )
    .into_diagnostic()
    .wrap_err("scan")?;
    assert!(records.is_empty());
    Ok(())
}

#[test]
fn skip_crates_does_not_invoke_runner() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src")).into_diagnostic()?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = \"skip_me_creusot\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .into_diagnostic()?;
    fs::write(fixture.path().join("src/lib.rs"), "pub fn ok() {}\n").into_diagnostic()?;
    fs::write(
        fixture.path().join("cordial.toml"),
        "[creusot_diagnostics]\nskip_crates = [\"skip_me_creusot\"]\n",
    )
    .into_diagnostic()?;

    let policy = load_cordial_config(fixture.path(), fixture.path());
    let records = scan_crate_creusot_diagnostics(
        fixture.path(),
        "skip_me_creusot",
        policy.creusot_diagnostics(),
    )
    .into_diagnostic()
    .wrap_err("scan")?;
    assert!(records.is_empty());
    Ok(())
}

#[cfg(unix)]
#[test]
fn session_writes_checklist_from_injected_creusot() -> miette::Result<()> {
    cordial::init_tracing();
    use std::os::unix::fs::PermissionsExt;
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let crate_root = fixture.path().join("canary_creusot");
    fs::create_dir_all(crate_root.join("src")).into_diagnostic()?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"canary_creusot\"]\nresolver = \"3\"\n",
    )
    .into_diagnostic()?;
    fs::write(
        crate_root.join("Cargo.toml"),
        "[package]\nname = \"canary_creusot\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .into_diagnostic()?;
    fs::write(crate_root.join("src/lib.rs"), "pub fn ok() {}\n").into_diagnostic()?;

    let fake = fixture.path().join("fake-creusot");
    let script = format!("#!/bin/sh\ncat \"{}\"\nexit 1\n", canary_path().display());
    fs::write(&fake, script).into_diagnostic()?;
    let mut perms = fs::metadata(&fake).into_diagnostic()?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&fake, perms).into_diagnostic()?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;

    let guard = CREUSOT_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let previous = std::env::var_os("CORDIAL_CREUSOT");
    unsafe {
        std::env::set_var("CORDIAL_CREUSOT", &fake);
    }
    let outcome = {
        let session = SessionBuilder::new(&crate_root)
            .with_store_root(store.path())
            .register(&CREUSOT_DIAGNOSTICS_ETIQUETTE)
            .build();
        session.run(&RunAll)
    };
    match previous {
        Some(value) => unsafe {
            std::env::set_var("CORDIAL_CREUSOT", value);
        },
        None => unsafe {
            std::env::remove_var("CORDIAL_CREUSOT");
        },
    }
    drop(guard);
    let outcome = outcome.into_diagnostic().wrap_err("session run")?;
    assert_eq!(outcome.findings().count(), 2);

    let findings_dir = store.path().join("findings");
    let csv = fs::read_to_string(findings_dir.join("creusot-diagnostics.csv"))
        .into_diagnostic()
        .wrap_err("csv")?;
    assert!(csv.contains("CREUSOT-DIAGNOSTIC-001"));
    assert!(csv.contains("CREUSOT-DIAGNOSTIC-002"));
    assert!(csv.contains("unused variable"));
    assert!(csv.contains("postcondition"));

    let checklist = fs::read_to_string(findings_dir.join("creusot-diagnostics.checklist.md"))
        .into_diagnostic()
        .wrap_err("checklist")?;
    assert!(checklist.contains("**Open items:** 2"));
    assert!(checklist.contains("cargo creusot prove"));

    let summary = fs::read_to_string(findings_dir.join("creusot-diagnostics-summary.md"))
        .into_diagnostic()
        .wrap_err("summary")?;
    assert!(summary.contains("**2** Creusot diagnostics"));
    Ok(())
}

#[test]
fn dogfood_cordial_is_not_a_creusot_target() {
    cordial::init_tracing();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    assert!(!crate_is_creusot_target(root));
}

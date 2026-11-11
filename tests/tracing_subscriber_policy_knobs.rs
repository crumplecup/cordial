//! The policy knobs: turning rules off, and skipping the lib, the bin, or program lints.

use cordial::{
    Disposition, RunAll, Session, SessionBuilder, SubscriberRuleId, TRACING_ETIQUETTE,
    TracingSubscriberPolicy,
};
use miette::{IntoDiagnostic, WrapErr};
use std::fs;

#[path = "support/tracing_subscriber_fixture.rs"]
mod tracing_subscriber_fixture;

use tracing_subscriber_fixture::{has, scan, subscriber_records, write_lib_bin_test};

#[test]
fn knobs_off_silence_all_five() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_lib_bin_test(
        "pub fn unused() {}\n",
        r#"
fn main() {
    tracing_subscriber::fmt().init();
}
"#,
        Some("#[test] fn it_works() {}\n"),
    )?;
    let policy = TracingSubscriberPolicy::new(false, false, false, false, false, Vec::new());
    let records = subscriber_records(fixture.path(), "fixture", &policy, false).wrap_err("scan")?;
    assert!(
        records.is_empty(),
        "all knobs off must be silent: {:?}",
        records
            .iter()
            .map(|record| record.rule_id())
            .collect::<Vec<_>>()
    );
    Ok(())
}

#[test]
fn lib_only_skips_main() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    fs::write(fixture.path().join("src/lib.rs"), "pub fn unused() {}\n")
        .into_diagnostic()
        .wrap_err("write lib")?;
    let rules = scan(fixture.path(), "fixture", false)?;
    assert!(
        !has(&rules, SubscriberRuleId::Main),
        "lib-only has no MAIN: {rules:?}"
    );
    Ok(())
}

#[test]
fn bin_only_skips_lib() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    fs::write(
        fixture.path().join("src/main.rs"),
        r#"
fn init_tracing() {
    let _ = tracing_subscriber::fmt().try_init();
}
fn main() {
    init_tracing();
}
"#,
    )
    .into_diagnostic()
    .wrap_err("write main")?;
    let rules = scan(fixture.path(), "fixture", false)?;
    assert!(
        !has(&rules, SubscriberRuleId::Lib),
        "bin-only has no library to host the helper: {rules:?}"
    );
    assert!(
        !has(&rules, SubscriberRuleId::Main),
        "main calls helper: {rules:?}"
    );
    Ok(())
}

#[test]
fn skip_program_lints_silences_main_and_test() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_lib_bin_test(
        "pub fn unused() {}\n",
        "fn main() {}\n",
        Some("#[test] fn it_works() {}\n"),
    )?;
    let rules = scan(fixture.path(), "fixture", true)?;
    assert!(
        !has(&rules, SubscriberRuleId::Main),
        "skip-policy crate skips MAIN: {rules:?}"
    );
    assert!(
        !has(&rules, SubscriberRuleId::Test),
        "skip-policy crate skips TEST: {rules:?}"
    );
    Ok(())
}

#[test]
fn session_writes_subscriber_checklist_not_instrument_rows() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_lib_bin_test("pub fn unused() {}\n", "fn main() {}\n", None)?;
    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&*TRACING_ETIQUETTE)
        .build();
    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    let subscriber: Vec<_> = outcome
        .findings()
        .filter(|finding| {
            finding.disposition() == Disposition::Open
                && SubscriberRuleId::is_subscriber_rule(finding.rule().id())
        })
        .collect();
    assert!(
        subscriber
            .iter()
            .any(|finding| finding.rule().id() == SubscriberRuleId::Main.as_str()),
        "session should emit MAIN"
    );

    let findings_dir = store.path().join("findings");
    let instrument = fs::read_to_string(findings_dir.join("tracing-instrument.checklist.md"))
        .into_diagnostic()
        .wrap_err("instrument checklist")?;
    assert!(
        !instrument.contains("TRACING-SUBSCRIBER-"),
        "instrument checklist must not swallow subscriber rows: {instrument}"
    );
    let checklist = fs::read_to_string(findings_dir.join("tracing-subscriber.checklist.md"))
        .into_diagnostic()
        .wrap_err("subscriber checklist")?;
    assert!(checklist.contains("TRACING-SUBSCRIBER-MAIN"));
    assert!(checklist.contains("**Open items:**"));
    Ok(())
}

#[test]
fn skip_crate_config_skips_main() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_lib_bin_test("pub fn unused() {}\n", "fn main() {}\n", None)?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = \"fixture_verus\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .into_diagnostic()
    .wrap_err("manifest")?;
    fs::write(
        fixture.path().join("cordial.toml"),
        "[tracing]\napply_skip_crates = [\"fixture_verus\"]\n",
    )
    .into_diagnostic()
    .wrap_err("config")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .with_store_home(store.path())
        .register(&*TRACING_ETIQUETTE)
        .build();
    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    let main = outcome.findings().any(|finding| {
        finding.disposition() == Disposition::Open
            && finding.rule().id() == SubscriberRuleId::Main.as_str()
    });
    assert!(!main, "apply_skip_crates must skip MAIN");
    Ok(())
}

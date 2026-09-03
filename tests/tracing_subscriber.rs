use miette::{IntoDiagnostic, WrapErr};
use std::fs;
use std::path::Path;

use cordial::{
    Disposition, RunAll, Session, SessionBuilder, SubscriberRuleId, TRACING_ETIQUETTE,
    TracingSubscriberPolicy, scan_crate_tracing_subscriber,
};

const GOOD_HELPER: &str = r#"
pub fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();
}
"#;

fn write_lib_bin_test(
    lib: &str,
    main: &str,
    test: Option<&str>,
) -> miette::Result<tempfile::TempDir> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(fixture.path().join("src/lib.rs"), lib)
        .into_diagnostic()
        .wrap_err("write lib")?;
    fs::write(fixture.path().join("src/main.rs"), main)
        .into_diagnostic()
        .wrap_err("write main")?;
    if let Some(test) = test {
        fs::create_dir_all(fixture.path().join("tests"))
            .into_diagnostic()
            .wrap_err("tests dir")?;
        fs::write(fixture.path().join("tests/smoke.rs"), test)
            .into_diagnostic()
            .wrap_err("write test")?;
    }
    Ok(fixture)
}

fn scan(
    root: &Path,
    crate_name: &str,
    skip_program_lints: bool,
) -> miette::Result<Vec<SubscriberRuleId>> {
    let records = scan_crate_tracing_subscriber(
        root,
        crate_name,
        &TracingSubscriberPolicy::default(),
        skip_program_lints,
    )
    .into_diagnostic()
    .wrap_err("scan")?;
    Ok(records.into_iter().map(|record| record.rule_id()).collect())
}

fn has(rules: &[SubscriberRuleId], rule: SubscriberRuleId) -> bool {
    rules.contains(&rule)
}

fn scan_with_known_helpers(
    root: &Path,
    crate_name: &str,
    known_helper_paths: &[&str],
) -> miette::Result<Vec<SubscriberRuleId>> {
    let policy = TracingSubscriberPolicy::new(
        true,
        true,
        true,
        true,
        true,
        known_helper_paths.iter().map(|s| s.to_string()).collect(),
    );
    let records = scan_crate_tracing_subscriber(root, crate_name, &policy, false)
        .into_diagnostic()
        .wrap_err("scan")?;
    Ok(records.into_iter().map(|record| record.rule_id()).collect())
}

#[test]
fn missing_init_in_main_is_flagged() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_lib_bin_test("pub fn unused() {}\n", "fn main() {}\n", None)?;
    let rules = scan(fixture.path(), "fixture", false)?;
    assert!(
        has(&rules, SubscriberRuleId::Main),
        "expected MAIN: {rules:?}"
    );
    assert!(
        !has(&rules, SubscriberRuleId::Lib),
        "no init site outside lib: {rules:?}"
    );
    Ok(())
}

#[test]
fn test_without_helper_call_is_flagged() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_lib_bin_test(
        GOOD_HELPER,
        "fn main() { init_tracing(); }\n",
        Some("#[test] fn it_works() { let _ = 1; }\n"),
    )?;
    let rules = scan(fixture.path(), "fixture", false)?;
    assert!(
        has(&rules, SubscriberRuleId::Test),
        "expected TEST: {rules:?}"
    );
    assert!(
        !has(&rules, SubscriberRuleId::Main),
        "main calls helper: {rules:?}"
    );
    Ok(())
}

#[test]
fn helper_only_in_main_fails_lib() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_lib_bin_test(
        "pub fn unused() {}\n",
        r#"
fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();
}
fn main() {
    init_tracing();
}
"#,
        None,
    )?;
    let rules = scan(fixture.path(), "fixture", false)?;
    assert!(
        has(&rules, SubscriberRuleId::Lib),
        "helper in main.rs must be LIB: {rules:?}"
    );
    assert!(
        !has(&rules, SubscriberRuleId::Main),
        "main calls its local helper: {rules:?}"
    );
    Ok(())
}

#[test]
fn inline_init_in_main_satisfies_main_fails_lib() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_lib_bin_test(
        "pub fn unused() {}\n",
        r#"
fn main() {
    tracing_subscriber::fmt().init();
}
"#,
        None,
    )?;
    let rules = scan(fixture.path(), "fixture", false)?;
    assert!(
        !has(&rules, SubscriberRuleId::Main),
        "inline init satisfies MAIN: {rules:?}"
    );
    assert!(
        has(&rules, SubscriberRuleId::Lib),
        "inline init in main is LIB: {rules:?}"
    );
    assert!(
        has(&rules, SubscriberRuleId::Idempotent),
        "bare init() is IDEMPOTENT: {rules:?}"
    );
    assert!(
        has(&rules, SubscriberRuleId::RustLog),
        "no RUST_LOG fallback: {rules:?}"
    );
    Ok(())
}

#[test]
fn helper_in_lib_that_main_never_calls_fails_main() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_lib_bin_test(GOOD_HELPER, "fn main() {}\n", None)?;
    let rules = scan(fixture.path(), "fixture", false)?;
    assert!(
        has(&rules, SubscriberRuleId::Main),
        "main never calls helper: {rules:?}"
    );
    assert!(
        !has(&rules, SubscriberRuleId::Lib),
        "helper is in lib: {rules:?}"
    );
    Ok(())
}

#[test]
fn good_helper_called_from_main_and_test_is_clean() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_lib_bin_test(
        GOOD_HELPER,
        "fn main() { init_tracing(); }\n",
        Some("#[test] fn it_works() { init_tracing(); }\n"),
    )?;
    let rules = scan(fixture.path(), "fixture", false)?;
    assert!(
        rules.is_empty(),
        "documented helper + callers should be clean: {rules:?}"
    );
    Ok(())
}

#[test]
fn init_without_once_fails_idempotent() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_lib_bin_test(
        r#"
pub fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
}
"#,
        "fn main() { init_tracing(); }\n",
        None,
    )?;
    let rules = scan(fixture.path(), "fixture", false)?;
    assert!(
        has(&rules, SubscriberRuleId::Idempotent),
        "bare init() must be IDEMPOTENT: {rules:?}"
    );
    assert!(
        !has(&rules, SubscriberRuleId::RustLog),
        "try_from_default_env + unwrap_or_else is enough: {rules:?}"
    );
    Ok(())
}

#[test]
fn from_default_env_without_fallback_fails_rust_log() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_lib_bin_test(
        r#"
pub fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();
}
"#,
        "fn main() { init_tracing(); }\n",
        None,
    )?;
    let rules = scan(fixture.path(), "fixture", false)?;
    assert!(
        has(&rules, SubscriberRuleId::RustLog),
        "from_default_env alone must be RUST-LOG: {rules:?}"
    );
    assert!(
        !has(&rules, SubscriberRuleId::Idempotent),
        "try_init is idempotent: {rules:?}"
    );
    Ok(())
}

#[test]
fn rust_log_env_var_plus_fallback_is_ok() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_lib_bin_test(
        r#"
pub fn init_tracing() {
    let _ = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into());
    let _ = tracing_subscriber::fmt().try_init();
}
"#,
        "fn main() { init_tracing(); }\n",
        None,
    )?;
    let rules = scan(fixture.path(), "fixture", false)?;
    assert!(
        !has(&rules, SubscriberRuleId::RustLog),
        "RUST_LOG literal + unwrap_or_else: {rules:?}"
    );
    Ok(())
}

#[test]
fn init_wrapped_in_once_is_idempotent() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_lib_bin_test(
        r#"
pub fn init_tracing() {
    static START: std::sync::Once = std::sync::Once::new();
    START.call_once(|| {
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
            )
            .init();
    });
}
"#,
        "fn main() { init_tracing(); }\n",
        None,
    )?;
    let rules = scan(fixture.path(), "fixture", false)?;
    assert!(
        !has(&rules, SubscriberRuleId::Idempotent),
        "Once + init() is idempotent: {rules:?}"
    );
    Ok(())
}

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
    let records = scan_crate_tracing_subscriber(fixture.path(), "fixture", &policy, false)
        .into_diagnostic()
        .wrap_err("scan")?;
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
        .register(&TRACING_ETIQUETTE)
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
        .register(&TRACING_ETIQUETTE)
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

/// Real-world regression: a shared helper defined in one crate
/// (`amenable_core::init_tracing`) and called from a sibling crate's
/// `main`/`#[test]` is invisible to a single-crate scan by construction --
/// this crate's own tree never contains the helper's defining body. Without
/// `known_helper_paths` naming it, every such call site is wrongly flagged
/// even though it correctly installs the subscriber.
#[test]
fn cross_crate_helper_without_known_helper_paths_is_flagged() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_lib_bin_test(
        "pub fn unused() {}\n",
        "fn main() { other_crate::init_tracing(); }\n",
        Some("#[test] fn it_works() { other_crate::init_tracing(); let _ = 1; }\n"),
    )?;
    let rules = scan(fixture.path(), "fixture", false)?;
    assert!(
        has(&rules, SubscriberRuleId::Main) && has(&rules, SubscriberRuleId::Test),
        "a call this crate can't see the body of must not be silently trusted \
         without config: {rules:?}"
    );
    Ok(())
}

#[test]
fn cross_crate_helper_named_in_known_helper_paths_is_trusted() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_lib_bin_test(
        "pub fn unused() {}\n",
        "fn main() { other_crate::init_tracing(); }\n",
        Some("#[test] fn it_works() { other_crate::init_tracing(); let _ = 1; }\n"),
    )?;
    let rules = scan_with_known_helpers(fixture.path(), "fixture", &["other_crate::init_tracing"])?;
    assert!(
        rules.is_empty(),
        "a configured cross-crate helper should satisfy MAIN/TEST/RUST_LOG/idempotent \
         all at once: {rules:?}"
    );
    Ok(())
}

#[test]
fn known_helper_paths_matches_bare_last_segment_too() -> miette::Result<()> {
    cordial::init_tracing();
    // An unqualified call (the helper imported via `use`) still names the
    // same last segment as the configured fully-qualified path.
    let fixture = write_lib_bin_test(
        "pub fn unused() {}\n",
        "use other_crate::init_tracing;\nfn main() { init_tracing(); }\n",
        None,
    )?;
    let rules = scan_with_known_helpers(fixture.path(), "fixture", &["other_crate::init_tracing"])?;
    assert!(
        !has(&rules, SubscriberRuleId::Main),
        "unqualified call to the same helper should still be trusted: {rules:?}"
    );
    Ok(())
}

#[test]
fn dogfood_cordial_subscriber_policy_is_clean() -> miette::Result<()> {
    cordial::init_tracing();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let rules = scan(root, "cordial", false)?;
    assert!(
        rules.is_empty(),
        "cordial should satisfy its own subscriber policy: {rules:?}"
    );
    Ok(())
}

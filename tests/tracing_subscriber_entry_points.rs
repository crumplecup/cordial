//! Where the subscriber is installed: the bin, the lib helper, and the integration tests that call it.

use cordial::SubscriberRuleId;

#[path = "support/tracing_subscriber_fixture.rs"]
mod tracing_subscriber_fixture;

use tracing_subscriber_fixture::{has, scan, write_lib_bin_test};

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
fn helper_that_delegates_to_installer_is_clean() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_lib_bin_test(
        r#"
pub fn init_tracing() {
    init_tracing_with_default("info");
}

fn init_tracing_with_default(default_filter: &str) {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(default_filter));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}
"#,
        "fn main() { init_tracing(); }\n",
        Some("#[test] fn it_works() { init_tracing(); }\n"),
    )?;
    let rules = scan(fixture.path(), "fixture", false)?;
    assert!(
        rules.is_empty(),
        "delegating helper should be trusted transitively: {rules:?}"
    );
    Ok(())
}

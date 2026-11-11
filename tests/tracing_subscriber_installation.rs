//! How the subscriber is installed: once-only, and an env filter with a fallback.

use cordial::SubscriberRuleId;

#[path = "support/tracing_subscriber_fixture.rs"]
mod tracing_subscriber_fixture;

use tracing_subscriber_fixture::{has, scan, write_lib_bin_test};

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

//! A subscriber is installed by `tracing`'s own functions, and a fallback is the
//! standard library's: a local function or method with the same name is neither.

use cordial::SubscriberRuleId;

#[path = "support/tracing_subscriber_fixture.rs"]
mod tracing_subscriber_fixture;

use tracing_subscriber_fixture::{has, scan, write_lib_bin_test};

const LOCAL_INIT_MAIN: &str =
    include_str!("fixtures/tracing_subscriber_identity/local_init_main.rs.txt");
const LOOKALIKE_FALLBACK_LIB: &str =
    include_str!("fixtures/tracing_subscriber_identity/lookalike_fallback_lib.rs.txt");
const CLOSURE_INSTALL_LIB: &str =
    include_str!("fixtures/tracing_subscriber_identity/closure_install_lib.rs.txt");

#[test]
fn a_local_function_named_init_installs_nothing() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_lib_bin_test("pub fn unused() {}\n", LOCAL_INIT_MAIN, None)?;
    let rules = scan(fixture.path(), "fixture", false)?;
    assert!(
        !has(&rules, SubscriberRuleId::Lib),
        "main installs nothing, so it does not install outside the library: {rules:?}"
    );
    Ok(())
}

#[test]
fn a_local_method_named_unwrap_or_else_is_not_the_standard_fallback() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_lib_bin_test(LOOKALIKE_FALLBACK_LIB, "fn main() {}\n", None)?;
    let rules = scan(fixture.path(), "fixture", false)?;
    assert!(has(&rules, SubscriberRuleId::RustLog), "{rules:?}");
    Ok(())
}

#[test]
fn an_install_inside_a_call_once_closure_counts() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_lib_bin_test(CLOSURE_INSTALL_LIB, "fn main() {}\n", None)?;
    let rules = scan(fixture.path(), "fixture", false)?;
    assert!(!has(&rules, SubscriberRuleId::Idempotent), "{rules:?}");
    Ok(())
}

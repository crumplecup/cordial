//! Helpers from other crates: trusted when named in `known_helper_paths`, flagged otherwise.

use cordial::{SubscriberRuleId, TracingSubscriberPolicy};
use miette::WrapErr;
use std::path::Path;

#[path = "support/tracing_subscriber_fixture.rs"]
mod tracing_subscriber_fixture;

use tracing_subscriber_fixture::{has, scan, subscriber_records, write_lib_bin_test};

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
    let records = subscriber_records(root, crate_name, &policy, false).wrap_err("scan")?;
    Ok(records.into_iter().map(|record| record.rule_id()).collect())
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

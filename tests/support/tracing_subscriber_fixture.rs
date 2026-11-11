//! Fixture crates for the subscriber-policy tests: a lib, a bin and optionally an integration test,
//! scanned for the subscriber rules each one breaks.

use cordial::{IrView, SubscriberRuleId, TracingSubscriberPolicy, tracing_subscriber_findings};
use miette::{IntoDiagnostic, WrapErr};
use std::fs;
use std::path::Path;

#[path = "projected_crate.rs"]
mod projected_crate;

/// Subscriber findings from the loader's observability inventory for `root`.
pub fn subscriber_records(
    root: &Path,
    crate_name: &str,
    policy: &TracingSubscriberPolicy,
    skip_program_lints: bool,
) -> miette::Result<Vec<cordial::SubscriberSiteRecord>> {
    let inventory = projected_crate::projected_crate(root)?
        .source_observability()
        .into_diagnostic()?;
    tracing_subscriber_findings(&inventory, crate_name, policy, skip_program_lints)
        .into_diagnostic()
}
pub fn write_lib_bin_test(
    lib: &str,
    main: &str,
    test: Option<&str>,
) -> miette::Result<tempfile::TempDir> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
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

pub fn scan(
    root: &Path,
    crate_name: &str,
    skip_program_lints: bool,
) -> miette::Result<Vec<SubscriberRuleId>> {
    let records = subscriber_records(
        root,
        crate_name,
        &TracingSubscriberPolicy::default(),
        skip_program_lints,
    )
    .wrap_err("scan")?;
    Ok(records.into_iter().map(|record| record.rule_id()).collect())
}

pub fn has(rules: &[SubscriberRuleId], rule: SubscriberRuleId) -> bool {
    rules.contains(&rule)
}

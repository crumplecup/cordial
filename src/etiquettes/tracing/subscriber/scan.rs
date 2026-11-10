//! Graph-only policy for tracing-subscriber init policy.

use std::path::PathBuf;

use crate::config::TracingSubscriberPolicy;
use crate::error::CordialResult;
use crate::ir::ObservabilityFileKind as FileKind;

use super::types::{SubscriberRuleId, SubscriberSiteRecord};
use crate::ir::ObservabilityBody as InitBodyFacts;

use tracing::instrument;

#[derive(Debug, Clone)]
struct FnSite {
    name: String,
    kind: FileKind,
    is_main: bool,
    is_test: bool,
    file: PathBuf,
    line: u32,
    facts: InitBodyFacts,
    context: String,
}

/// Apply the subscriber-init policy to the loader-projected functions.
#[instrument(level = "debug", skip(inventory, policy), err(level = "warn"))]
pub fn findings_from_inventory(
    inventory: &crate::ir::SourceObservability,
    crate_name: &str,
    policy: &TracingSubscriberPolicy,
    skip_program_lints: bool,
) -> CordialResult<Vec<SubscriberSiteRecord>> {
    let mut sites = Vec::new();
    for file in inventory.files() {
        if crate::loader::path_has_fixtures(file.snapshot().path(), inventory.crate_root()) {
            continue;
        }
        for function in file.functions() {
            sites.push(FnSite {
                name: function.name().clone(),
                kind: file.kind(),
                is_test: file.kind() == FileKind::Test && function.test(),
                is_main: function.name() == "main" && file.kind() == FileKind::Bin,
                file: file.relative().clone(),
                line: function.line(),
                facts: function.body().clone(),
                context: function.context(crate_name),
            });
        }
    }
    findings_for_sites(&sites, inventory, policy, skip_program_lints)
}

#[instrument(level = "debug", skip(sites, policy, inventory), err(level = "warn"))]
fn findings_for_sites(
    sites: &[FnSite],
    inventory: &crate::ir::SourceObservability,
    policy: &TracingSubscriberPolicy,
    skip_program_lints: bool,
) -> CordialResult<Vec<SubscriberSiteRecord>> {
    let has_lib = inventory.has_lib();
    let has_bin = inventory.has_bin();

    let helper_names = trusted_helper_names(sites);
    let helper_refs: Vec<&str> = helper_names.iter().map(String::as_str).collect();

    let mut findings = Vec::new();
    for site in sites {
        if policy.init_in_main()
            && !skip_program_lints
            && has_bin
            && site.is_main
            && !site
                .facts
                .installs_or_delegates(policy.known_helper_paths())
            && !site.facts.calls_helper(&helper_refs)
        {
            findings.push(record(
                SubscriberRuleId::Main,
                site,
                "fn main never installs a tracing subscriber — call the library helper",
            )?);
        }
        if policy.init_in_tests()
            && !skip_program_lints
            && site.is_test
            && !site
                .facts
                .installs_or_delegates(policy.known_helper_paths())
            && !site.facts.calls_helper(&helper_refs)
        {
            findings.push(record(
                SubscriberRuleId::Test,
                site,
                "#[test] never installs a tracing subscriber — call the library helper",
            )?);
        }
        if policy.helper_in_lib()
            && has_lib
            && site.facts.calls_install()
            && matches!(site.kind, FileKind::Bin | FileKind::Test)
        {
            findings.push(record(
                SubscriberRuleId::Lib,
                site,
                "subscriber init lives outside the library — move it to one documented helper",
            )?);
        }
        if policy.rust_log_fallback() && site.facts.calls_install() && !site.facts.rust_log_ok() {
            findings.push(record(
                SubscriberRuleId::RustLog,
                site,
                "init helper must read RUST_LOG with a fallback (try_from_default_env + unwrap_or)",
            )?);
        }
        if policy.idempotent() && site.facts.calls_install() && !site.facts.idempotent_ok() {
            findings.push(record(
                SubscriberRuleId::Idempotent,
                site,
                "init helper uses init() without Once/OnceLock — use try_init() or wrap in Once",
            )?);
        }
    }

    findings.sort_by(|a, b| {
        a.file()
            .cmp(b.file())
            .then(a.line().cmp(&b.line()))
            .then(a.rule_id().as_str().cmp(b.rule_id().as_str()))
            .then(a.snippet().cmp(b.snippet()))
    });
    Ok(findings)
}

#[instrument(level = "debug", skip(sites))]
fn trusted_helper_names(sites: &[FnSite]) -> Vec<String> {
    let mut helper_names: Vec<String> = sites
        .iter()
        .filter(|site| site.facts.calls_install())
        .map(|site| site.name.clone())
        .collect();

    loop {
        let helper_refs: Vec<&str> = helper_names.iter().map(String::as_str).collect();
        let discovered: Vec<String> = sites
            .iter()
            .filter(|site| !helper_names.contains(&site.name))
            .filter(|site| site.facts.calls_helper(&helper_refs))
            .map(|site| site.name.clone())
            .collect();
        if discovered.is_empty() {
            break;
        }
        helper_names.extend(discovered);
    }

    helper_names
}

#[instrument(level = "debug", skip(rule_id, site))]
fn record(
    rule_id: SubscriberRuleId,
    site: &FnSite,
    snippet: &str,
) -> CordialResult<SubscriberSiteRecord> {
    SubscriberSiteRecord::builder()
        .rule_id(rule_id)
        .context(site.context.clone())
        .file(site.file.clone())
        .line(site.line)
        .snippet(snippet.to_string())
        .build()
}

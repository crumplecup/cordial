//! Graph-only policy for the binary error-boundary policy: a fallible
//! `fn main` must convert its error to a tracing warn/error emission
//! before the process boundary, not let it bubble to a crash.

use std::path::PathBuf;

use crate::config::TracingBoundaryPolicy;
use crate::error::CordialResult;
use crate::ir::ObservabilityFileKind as FileKind;

use super::types::{BoundaryRuleId, BoundarySiteRecord};
use crate::ir::ObservabilityBody as BoundaryBodyFacts;

use tracing::instrument;

#[derive(Debug, Clone)]
struct FnSite {
    is_main: bool,
    file: PathBuf,
    line: u32,
    facts: BoundaryBodyFacts,
    context: String,
    location: crate::FunctionLocation,
}

/// Apply the binary error-boundary policy to the loader-projected functions.
/// Every function in the crate is considered (not just `fn main`) so a helper
/// `main` delegates to (e.g. `Cli::act`) can be recognized as already
/// reporting its own errors.
#[instrument(level = "debug", skip(inventory, policy), err(level = "warn"))]
#[hotpath::measure]
pub fn findings_from_inventory(
    inventory: &crate::ir::SourceObservability,
    crate_name: &str,
    policy: &TracingBoundaryPolicy,
    skip_program_lints: bool,
) -> CordialResult<Vec<BoundarySiteRecord>> {
    let mut sites = Vec::new();
    for file in inventory.files() {
        if crate::loader::path_has_fixtures(file.snapshot().path(), inventory.crate_root()) {
            continue;
        }
        for function in file.functions() {
            sites.push(FnSite {
                is_main: function.name() == "main" && file.kind() == FileKind::Bin,
                file: file.relative().clone(),
                line: function.line(),
                facts: function.body().clone(),
                context: function.context(crate_name),
                location: function.location(file.snapshot()),
            });
        }
    }
    findings_for_sites(&sites, inventory, policy, skip_program_lints)
}

#[instrument(level = "debug", skip(sites, policy, inventory), err(level = "warn"))]
#[hotpath::measure]
fn findings_for_sites(
    sites: &[FnSite],
    inventory: &crate::ir::SourceObservability,
    policy: &TracingBoundaryPolicy,
    skip_program_lints: bool,
) -> CordialResult<Vec<BoundarySiteRecord>> {
    let has_bin = inventory.has_bin();

    let safe_names: Vec<crate::FunctionLocation> = sites
        .iter()
        .filter(|site| {
            site.facts.is_fallible() && site.facts.reports_errors(policy.known_helper_paths())
        })
        .map(|site| site.location.clone())
        .collect();

    let mut findings = Vec::new();
    for site in sites {
        if policy.main_reports_errors()
            && !skip_program_lints
            && has_bin
            && site.is_main
            && site.facts.is_fallible()
            && !site.facts.reports_errors(policy.known_helper_paths())
            && !site.facts.calls_helper(&safe_names)
        {
            findings.push(record(
                site,
                "fallible fn main never converts its error to a tracing warn/error emission \
                 before returning — add #[instrument(err(...))] or emit tracing::warn!/error! \
                 on the error path",
            )?);
        }
    }

    findings.sort_by(|a, b| {
        a.file()
            .cmp(b.file())
            .then(a.line().cmp(&b.line()))
            .then(a.rule_id().as_str().cmp(b.rule_id().as_str()))
    });
    Ok(findings)
}

#[instrument(level = "debug", skip(site), err(level = "warn"))]
#[hotpath::measure]
fn record(site: &FnSite, snippet: &str) -> CordialResult<BoundarySiteRecord> {
    BoundarySiteRecord::builder()
        .rule_id(BoundaryRuleId::MainSilent)
        .context(site.context.clone())
        .file(site.file.clone())
        .line(site.line)
        .snippet(snippet.to_string())
        .build()
}

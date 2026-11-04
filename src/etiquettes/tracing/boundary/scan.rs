//! Graph-only policy for the binary error-boundary policy: a fallible
//! `fn main` must convert its error to a tracing warn/error emission
//! before the process boundary, not let it bubble to a crash.

use std::path::{Path, PathBuf};

use crate::config::TracingBoundaryPolicy;
use crate::error::CordialResult;
use crate::ir::ObservabilityFileKind as FileKind;
use crate::loader::SourceLoadView;

use super::types::{BoundaryRuleId, BoundarySiteRecord};
use crate::ir::ObservabilityBody as BoundaryBodyFacts;

use tracing::instrument;

#[derive(Debug, Clone)]
struct FnSite {
    name: String,
    is_main: bool,
    file: PathBuf,
    line: u32,
    facts: BoundaryBodyFacts,
    context: String,
}

/// Scan lib, bins, and `tests/` for the binary error-boundary policy.
/// Every function in the crate is scanned (not just `fn main`) so a
/// helper `main` delegates to (e.g. `Cli::act`) can be recognized as
/// already reporting its own errors — mirrors
/// [`super::super::subscriber::scan_crate_tracing_subscriber`]'s
/// helper-name delegation strategy.
#[instrument(level = "debug", skip(policy), err(level = "warn"))]
pub fn scan_crate_tracing_boundary(
    crate_root: &Path,
    crate_name: &str,
    policy: &TracingBoundaryPolicy,
    skip_program_lints: bool,
) -> CordialResult<Vec<BoundarySiteRecord>> {
    let source = SourceLoadView::for_standalone_scan();
    let inventory = crate::ir::SourceObservability::collect(&source, crate_root)?;
    findings_from_inventory(&inventory, crate_name, policy, skip_program_lints)
}

#[instrument(level = "debug", skip(inventory, policy), err(level = "warn"))]
pub(super) fn findings_from_inventory(
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
                name: function.name().clone(),
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
    policy: &TracingBoundaryPolicy,
    skip_program_lints: bool,
) -> CordialResult<Vec<BoundarySiteRecord>> {
    let has_bin = inventory.has_bin();

    let safe_names: Vec<&str> = sites
        .iter()
        .filter(|site| {
            site.facts.is_fallible() && site.facts.reports_errors(policy.known_helper_paths())
        })
        .map(|site| site.name.as_str())
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
fn record(site: &FnSite, snippet: &str) -> CordialResult<BoundarySiteRecord> {
    BoundarySiteRecord::builder()
        .rule_id(BoundaryRuleId::MainSilent)
        .context(site.context.clone())
        .file(site.file.clone())
        .line(site.line)
        .snippet(snippet.to_string())
        .build()
}

//! Merge per-crate antipattern scans (syn, contract bounds, version-in-member).

use std::path::Path;

use crate::config::{ErrorFieldStrategy, StaticRefStrategy};
use crate::error::CordialResult;
use crate::loader::SourceLoadView;

use super::contract_bounds::{fetch_contract_records, scan_crate_contract_bounds_with_source};
use super::scan::scan_shared_crate_trees;
use super::types::AntipatternSiteRecord;
use super::version_in_member::scan_workspace_version_in_member;

use tracing::instrument;
/// Scan one crate for antipatterns.
#[instrument(level = "debug", err(level = "warn"))]
pub fn scan_crate_antipatterns(
    crate_root: &Path,
    crate_name: &str,
    workspace_root: &Path,
    store_root: &Path,
) -> CordialResult<Vec<AntipatternSiteRecord>> {
    let source = SourceLoadView::for_standalone_scan();
    scan_shared_crate_antipatterns(
        &source,
        crate_root,
        crate_name,
        workspace_root,
        store_root,
        StaticRefStrategy::default(),
        ErrorFieldStrategy::default(),
    )
}

/// Scan one crate for antipatterns using a static-reference remediation strategy.
#[instrument(level = "debug", skip(static_ref_strategy))]
pub fn scan_crate_antipatterns_with_static_ref_strategy(
    crate_root: &Path,
    crate_name: &str,
    workspace_root: &Path,
    store_root: &Path,
    static_ref_strategy: StaticRefStrategy,
) -> CordialResult<Vec<AntipatternSiteRecord>> {
    let source = SourceLoadView::for_standalone_scan();
    scan_shared_crate_antipatterns(
        &source,
        crate_root,
        crate_name,
        workspace_root,
        store_root,
        static_ref_strategy,
        ErrorFieldStrategy::default(),
    )
}

#[instrument(
    level = "debug",
    skip(source, static_ref_strategy, error_field_strategy),
    err(level = "warn")
)]
pub(super) fn scan_shared_crate_antipatterns(
    source: &SourceLoadView,
    crate_root: &Path,
    crate_name: &str,
    workspace_root: &Path,
    store_root: &Path,
    static_ref_strategy: StaticRefStrategy,
    error_field_strategy: ErrorFieldStrategy,
) -> CordialResult<Vec<AntipatternSiteRecord>> {
    let findings = scan_shared_crate_trees(
        source,
        crate_root,
        static_ref_strategy,
        error_field_strategy,
    )?;
    finish_crate_antipattern_scan(
        findings,
        crate_root,
        crate_name,
        workspace_root,
        store_root,
        source,
    )
}

#[instrument(level = "debug", skip(findings, source), err(level = "warn"))]
fn finish_crate_antipattern_scan(
    mut findings: Vec<AntipatternSiteRecord>,
    crate_root: &Path,
    crate_name: &str,
    workspace_root: &Path,
    store_root: &Path,
    source: &SourceLoadView,
) -> CordialResult<Vec<AntipatternSiteRecord>> {
    let registry = fetch_contract_records(workspace_root, store_root);
    findings.extend(scan_crate_contract_bounds_with_source(
        crate_root, crate_name, &registry, source,
    )?);

    let version_by_crate = scan_workspace_version_in_member(workspace_root)?;
    if let Some(version_findings) = version_by_crate.get(crate_name) {
        findings.extend(version_findings.iter().cloned());
    }

    findings.sort_by(|a, b| {
        a.file()
            .cmp(b.file())
            .then(a.line().cmp(&b.line()))
            .then(a.context().cmp(b.context()))
            .then(a.snippet().cmp(b.snippet()))
    });

    Ok(findings)
}

//! Merge per-crate antipattern scans (syn, contract bounds, version-in-member).

use std::collections::HashMap;
use std::path::Path;

use crate::config::{ErrorFieldStrategy, StaticRefStrategy};
use crate::error::CordialResult;
use crate::loader::SourceLoadView;

use super::contract_bounds::{fetch_contract_records, scan_crate_contract_bounds_with_source};
use super::scan::scan_shared_crate_trees;
use super::types::AntipatternSiteRecord;
use super::version_in_member::scan_workspace_version_in_member;
use tracing::instrument;

/// Workspace evidence shared by every member's antipattern inventory.
#[derive(Debug)]
pub struct AntipatternWorkspaceFacts {
    registry: Vec<super::contract_bounds::ContractRecordDump>,
    version_by_crate: HashMap<String, Vec<AntipatternSiteRecord>>,
}

impl AntipatternWorkspaceFacts {
    #[instrument(level = "debug")]
    pub(crate) fn load(workspace_root: &Path, store_root: &Path) -> CordialResult<Self> {
        Ok(Self {
            registry: fetch_contract_records(workspace_root, store_root),
            version_by_crate: scan_workspace_version_in_member(workspace_root)?,
        })
    }
}

/// Scan one crate for antipatterns.
#[instrument(level = "debug", err(level = "warn"))]
pub fn scan_crate_antipatterns(
    crate_root: &Path,
    crate_name: &str,
    workspace_root: &Path,
    store_root: &Path,
) -> CordialResult<Vec<AntipatternSiteRecord>> {
    let source = SourceLoadView::for_standalone_scan();
    let facts = AntipatternWorkspaceFacts::load(workspace_root, store_root)?;
    scan_shared_crate_antipatterns(
        &source,
        crate_root,
        crate_name,
        &facts,
        StaticRefStrategy::default(),
        ErrorFieldStrategy::default(),
        &HashMap::new(),
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
    let facts = AntipatternWorkspaceFacts::load(workspace_root, store_root)?;
    scan_shared_crate_antipatterns(
        &source,
        crate_root,
        crate_name,
        &facts,
        static_ref_strategy,
        ErrorFieldStrategy::default(),
        &HashMap::new(),
    )
}

#[instrument(
    level = "debug",
    skip(source, facts, static_ref_strategy, error_field_strategy, skip_paths),
    err(level = "warn")
)]
pub(super) fn scan_shared_crate_antipatterns(
    source: &SourceLoadView,
    crate_root: &Path,
    crate_name: &str,
    facts: &AntipatternWorkspaceFacts,
    static_ref_strategy: StaticRefStrategy,
    error_field_strategy: ErrorFieldStrategy,
    skip_paths: &HashMap<String, Vec<String>>,
) -> CordialResult<Vec<AntipatternSiteRecord>> {
    let findings = scan_shared_crate_trees(
        source,
        crate_root,
        static_ref_strategy,
        error_field_strategy,
        skip_paths,
    )?;
    finish_crate_antipattern_scan(findings, crate_root, crate_name, facts, source)
}

#[instrument(level = "debug", skip(findings, facts, source), err(level = "warn"))]
fn finish_crate_antipattern_scan(
    mut findings: Vec<AntipatternSiteRecord>,
    crate_root: &Path,
    crate_name: &str,
    facts: &AntipatternWorkspaceFacts,
    source: &SourceLoadView,
) -> CordialResult<Vec<AntipatternSiteRecord>> {
    findings.extend(scan_crate_contract_bounds_with_source(
        crate_root,
        crate_name,
        &facts.registry,
        source,
    )?);

    if let Some(version_findings) = facts.version_by_crate.get(crate_name) {
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

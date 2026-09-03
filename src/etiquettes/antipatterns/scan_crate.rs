//! Merge per-crate antipattern scans (syn, contract bounds, version-in-member).

use std::path::Path;

use crate::error::CordialResult;

use super::contract_bounds::{fetch_contract_records, scan_crate_contract_bounds};
use super::scan::scan_crate_trees;
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
    let mut findings = scan_crate_trees(crate_root)?;

    let registry = fetch_contract_records(workspace_root, store_root);
    findings.extend(scan_crate_contract_bounds(
        crate_root, crate_name, &registry,
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

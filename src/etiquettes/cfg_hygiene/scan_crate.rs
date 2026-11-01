//! Per-crate cfg-hygiene scan: every cfg name occurrence in the crate's
//! `src/` tree, cross-referenced against [`declared`] to produce
//! UNEXPECTED-CFG-001 and CFG-VERIFIER-MISMATCH-001 records.

use std::path::Path;

use crate::config::CfgHygieneThresholds;
use crate::error::CordialResult;
use crate::loader::SourceLoadView;

use super::declared::{all_verifier_names, expected_verifier_for};
use super::scan::CfgNameOccurrence;
use super::types::{CfgHygieneRuleId, CfgHygieneSiteRecord};

use tracing::instrument;
/// Scan one crate for cfg hygiene.
#[instrument(level = "debug", skip(thresholds), err(level = "warn"))]
pub fn scan_crate_cfg_hygiene(
    crate_root: &Path,
    crate_name: &str,
    workspace_root: &Path,
    thresholds: &CfgHygieneThresholds,
) -> CordialResult<Vec<CfgHygieneSiteRecord>> {
    let source =
        SourceLoadView::for_standalone_scan().with_workspace_root(workspace_root.to_path_buf());
    let inventory = crate::ir::SourcePolicyInventory::collect(&source, crate_root)?;
    records_for_inventory(&inventory, crate_name, thresholds)
}

#[instrument(level = "debug", skip(inventory, thresholds), err(level = "warn"))]
pub(crate) fn records_for_inventory(
    inventory: &crate::ir::SourcePolicyInventory,
    crate_name: &str,
    thresholds: &CfgHygieneThresholds,
) -> CordialResult<Vec<CfgHygieneSiteRecord>> {
    let mut declared = inventory.known_cfg_names().clone();
    declared.extend(thresholds.extra_known_names().iter().cloned());
    let occurrences = inventory.cfg_names();
    let expected_verifier = expected_verifier_for(thresholds, crate_name);
    let verifier_names = all_verifier_names(thresholds);

    let mut records = Vec::new();
    for occurrence in occurrences {
        if !declared.contains(occurrence.name()) {
            records.push(record_for(occurrence, CfgHygieneRuleId::UnexpectedCfg001)?);
        }
        if let Some(expected) = expected_verifier
            && verifier_names.contains(occurrence.name().as_str())
            && occurrence.name() != expected
        {
            records.push(record_for(
                occurrence,
                CfgHygieneRuleId::CfgVerifierMismatch001,
            )?);
        }
    }

    records.sort_by(|a, b| {
        a.file()
            .cmp(b.file())
            .then(a.line().cmp(&b.line()))
            .then(a.cfg_name().cmp(b.cfg_name()))
            .then(a.rule_id().as_str().cmp(b.rule_id().as_str()))
    });
    Ok(records)
}

#[instrument(level = "debug", skip(occurrence, rule_id))]
fn record_for(
    occurrence: &CfgNameOccurrence,
    rule_id: CfgHygieneRuleId,
) -> CordialResult<CfgHygieneSiteRecord> {
    CfgHygieneSiteRecord::builder()
        .rule_id(rule_id)
        .cfg_name(occurrence.name().clone())
        .context(occurrence.context().clone())
        .file(occurrence.file().clone())
        .line(occurrence.line())
        .snippet(occurrence.snippet().clone())
        .build()
}

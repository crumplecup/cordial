//! Per-crate cfg-hygiene scan: every cfg name occurrence in the crate's
//! `src/` tree, cross-referenced against [`declared`] to produce
//! UNEXPECTED-CFG-001 and CFG-VERIFIER-MISMATCH-001 records.

use std::path::Path;

use crate::config::CfgHygieneThresholds;
use crate::error::CordialResult;

use super::declared::{all_verifier_names, declared_names_for_crate, expected_verifier_for};
use super::scan::{CfgNameOccurrence, scan_rust_source};
use super::types::{CfgHygieneRuleId, CfgHygieneSiteRecord};

use tracing::instrument;
#[instrument(level = "debug", skip(thresholds), err(level = "warn"))]
pub fn scan_crate_cfg_hygiene(
    crate_root: &Path,
    crate_name: &str,
    workspace_root: &Path,
    thresholds: &CfgHygieneThresholds,
) -> CordialResult<Vec<CfgHygieneSiteRecord>> {
    let src_root = crate_root.join("src");
    let occurrences = scan_source_tree(&src_root, crate_root)?;

    let declared = declared_names_for_crate(crate_root, workspace_root, thresholds);
    let expected_verifier = expected_verifier_for(thresholds, crate_name);
    let verifier_names = all_verifier_names(thresholds);

    let mut records = Vec::new();
    for occurrence in &occurrences {
        if !declared.contains(&occurrence.name) {
            records.push(record_for(occurrence, CfgHygieneRuleId::UnexpectedCfg001));
        }
        if let Some(expected) = expected_verifier
            && verifier_names.contains(occurrence.name.as_str())
            && occurrence.name != expected
        {
            records.push(record_for(
                occurrence,
                CfgHygieneRuleId::CfgVerifierMismatch001,
            ));
        }
    }

    records.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then(a.line.cmp(&b.line))
            .then(a.cfg_name.cmp(&b.cfg_name))
            .then(a.rule_id.as_str().cmp(b.rule_id.as_str()))
    });
    Ok(records)
}

#[instrument(level = "debug", skip(occurrence))]
fn record_for(occurrence: &CfgNameOccurrence, rule_id: CfgHygieneRuleId) -> CfgHygieneSiteRecord {
    CfgHygieneSiteRecord {
        rule_id,
        cfg_name: occurrence.name.clone(),
        context: occurrence.context.clone(),
        file: occurrence.file.clone(),
        line: occurrence.line,
        snippet: occurrence.snippet.clone(),
    }
}

#[instrument(level = "debug", err(level = "warn"))]
fn scan_source_tree(src_root: &Path, crate_root: &Path) -> CordialResult<Vec<CfgNameOccurrence>> {
    let mut occurrences = Vec::new();
    if !src_root.is_dir() {
        return Ok(occurrences);
    }

    for entry in walkdir::WalkDir::new(src_root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "rs") {
            continue;
        }
        let source = std::fs::read_to_string(path)?;
        occurrences.extend(scan_rust_source(&source, path, src_root, crate_root)?);
    }

    Ok(occurrences)
}

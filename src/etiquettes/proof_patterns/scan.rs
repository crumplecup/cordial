//! Extract soundness/proof-visibility facts from real `verus_syn` parses.

use std::path::Path;

use crate::error::CordialResult;
use crate::verus_ir::{VerusCrateIr, VerusFnFacts, VerusFnMode, VerusPublish};

use super::types::{ProofPatternKind, ProofPatternRecord};

use tracing::instrument;

/// Scan a crate's real `verus! { .. }` blocks for soundness-relevant
/// patterns: functions that are trusted rather than proven (`assume`/
/// `admit`/`external_body`/`uninterp`/`axiom`), and `broadcast` lemmas,
/// whose contribution to the total proof burden is invisible at call
/// sites. Reuses [`crate::verus_ir::scan_crate_verus_ir`] -- the same
/// real `verus_syn` parse `panics` already merges in for panic sites.
#[instrument(level = "debug", err(level = "warn"))]
pub fn scan_crate_proof_patterns(crate_root: &Path) -> CordialResult<Vec<ProofPatternRecord>> {
    let ir = crate::verus_ir::scan_crate_verus_ir(crate_root)?;
    Ok(proof_pattern_records(&ir, crate_root))
}

/// Convert every real proof-pattern signal `ir` carries into this
/// etiquette's own record shape -- one record per active kind per
/// function, since a function can carry more than one (e.g. `assume`
/// and `admit` in the same body).
#[instrument(level = "debug", skip(ir))]
fn proof_pattern_records(ir: &VerusCrateIr, crate_root: &Path) -> Vec<ProofPatternRecord> {
    ir.functions
        .iter()
        .flat_map(|function| function_records(function, crate_root))
        .collect()
}

#[instrument(level = "debug", skip(function))]
fn function_records(function: &VerusFnFacts, crate_root: &Path) -> Vec<ProofPatternRecord> {
    let context = format!("{}::{}", function.module_path, function.name);
    let file = function
        .span
        .file
        .strip_prefix(crate_root)
        .unwrap_or(&function.span.file)
        .to_path_buf();

    active_kinds(function)
        .into_iter()
        .map(|(kind, snippet)| ProofPatternRecord {
            kind,
            context: context.clone(),
            file: file.clone(),
            line: function.span.line,
            snippet: snippet.to_string(),
            cfg_test: function.cfg_test,
            tracked_params: function.tracked_params.clone(),
            recommends: function.recommends.clone(),
        })
        .collect()
}

/// Every [`ProofPatternKind`] `function` actively carries, paired with
/// the real syntax that triggered it.
#[instrument(level = "trace", skip(function), ret)]
fn active_kinds(function: &VerusFnFacts) -> Vec<(ProofPatternKind, &'static str)> {
    let mut kinds = Vec::new();
    if function.uses_assume {
        kinds.push((ProofPatternKind::Assume, "assume(..)"));
    }
    if function.uses_admit {
        kinds.push((ProofPatternKind::Admit, "admit()"));
    }
    if function.is_external_body {
        kinds.push((ProofPatternKind::ExternalBody, "#[verifier::external_body]"));
    }
    if matches!(function.publish, VerusPublish::Uninterp) {
        kinds.push((ProofPatternKind::Uninterp, "uninterp spec fn"));
    }
    if matches!(function.mode, VerusFnMode::ProofAxiom) {
        kinds.push((ProofPatternKind::Axiom, "axiom fn"));
    }
    if function.is_broadcast {
        kinds.push((ProofPatternKind::Broadcast, "broadcast proof fn"));
    }
    kinds
}

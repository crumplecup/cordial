//! Crate-tree scan for `pub mod` paths that do not earn their visibility.

use crate::error::CordialResult;

use super::types::{VisibilityRecord, VisibilityThresholds};

use tracing::instrument;
mod eval;
mod findings;
mod tree;

use eval::resolve_eval;
use findings::collect_findings;

/// Apply `thresholds` to the loader-projected module tree, without reading
/// source or resolving paths. The scanner never picks thresholds itself.
#[instrument(level = "debug", skip(facts, thresholds), err(level = "warn"))]
pub fn select_records(
    facts: &crate::ir::VisibilityFacts,
    thresholds: VisibilityThresholds,
) -> CordialResult<Vec<VisibilityRecord>> {
    let Some(root) = facts.root() else {
        return Ok(Vec::new());
    };
    let eval = resolve_eval(root, thresholds.clone());
    collect_findings(root, thresholds, eval)
}

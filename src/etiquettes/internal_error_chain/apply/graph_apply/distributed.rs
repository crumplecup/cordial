//! Plan a closed private error graph whose declarations span source files.
//!
//! Every declaration comes from the crate's shared parsed files through the
//! same identity index used by conversion-unit enrichment. Unknown edges,
//! hidden names, and unsupported lifetime changes leave the graph untouched.

use std::collections::BTreeMap;

use tracing::instrument;

use crate::config::ErrorFieldStrategy;
use crate::loader::SourceEditView;

use super::super::inventory::{CrateTypeIndex, ErrorEdgeTarget};
use super::ErrorGraphPlan;
use super::declarations::GraphDeclaration;

mod edits;
mod ownership;

type Declarations<'a> =
    BTreeMap<String, (&'a SourceEditView, &'a GraphDeclaration, ErrorEdgeTarget)>;

/// Retain per-file component ownership when one root reaches an external Kind
/// or source declaration. Rejected components carry no edits, preventing a
/// fallback field conversion from bypassing the graph's atomicity.
#[instrument(level = "debug", skip(root_file, files, strategy, index))]
pub(super) fn plan(
    root_file: &SourceEditView,
    files: &[SourceEditView],
    strategy: ErrorFieldStrategy,
    index: &CrateTypeIndex,
) -> Option<Vec<ErrorGraphPlan>> {
    let declarations = ownership::discover(root_file, files, index)?;
    Some(
        edits::plan(&declarations, files, strategy, index)
            .unwrap_or_else(|| ownership::rejected_plans(&declarations, strategy)),
    )
}

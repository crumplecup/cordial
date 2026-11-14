//! The crate-wide evidence apply plans from.
//!
//! Each phase is a function of the one before it and of the per-file evidence,
//! so the database can hold each as a query: the type index from the files'
//! facts, the conversion units from the index, the graph plans from the index,
//! the files' text and the field strategy. Nothing here touches the IR.

use tracing::instrument;

use crate::FileSnapshot;
use crate::config::ErrorFieldStrategy;
use crate::error::CordialResult;
use crate::ir::SourceErrorApply;
use crate::loader::SourceEditView;

use super::graph_apply::{ErrorGraphPlan, plan_conversion_graphs};
use super::inventory::{CrateTypeIndex, ErrorConversionUnit, conversion_units};

/// Everything apply reads about a crate before it plans any edit.
#[derive(Debug, derive_getters::Getters, derive_new::new)]
#[new(visibility = "pub(crate)")]
pub(crate) struct ApplyEvidence {
    source: SourceErrorApply,
    index: CrateTypeIndex,
    units: Vec<ErrorConversionUnit>,
    graphs: Vec<ErrorGraphPlan>,
}

/// The snapshots of the crate's files, in path order.
#[instrument(level = "trace", skip(source))]
pub(crate) fn snapshots(source: &SourceErrorApply) -> Vec<FileSnapshot> {
    source
        .files()
        .iter()
        .map(|file| file.snapshot().clone())
        .collect()
}

/// The crate-wide index of declarations apply resolves paths against.
#[instrument(level = "debug", skip(source), err(level = "warn"))]
#[hotpath::measure]
pub(crate) fn type_index(source: &SourceErrorApply) -> CordialResult<CrateTypeIndex> {
    CrateTypeIndex::new(&snapshots(source), source)
}

/// The conversion unit of every candidate field.
#[instrument(level = "debug", skip(source, index), err(level = "warn"))]
#[hotpath::measure]
pub(crate) fn units(
    source: &SourceErrorApply,
    index: &CrateTypeIndex,
) -> CordialResult<Vec<ErrorConversionUnit>> {
    conversion_units(&snapshots(source), index)
}

/// The whole-file plan of every private error graph, for `strategy`.
#[instrument(level = "debug", skip(files, index))]
#[hotpath::measure]
pub(crate) fn graphs(
    files: &[SourceEditView],
    strategy: ErrorFieldStrategy,
    index: &CrateTypeIndex,
) -> Vec<ErrorGraphPlan> {
    plan_conversion_graphs(files, strategy, index)
}

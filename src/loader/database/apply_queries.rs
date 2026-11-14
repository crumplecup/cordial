//! Error-apply evidence as queries over the per-file results.
//!
//! Apply plans from facts about the whole crate: an index of declarations, the
//! conversion unit of each candidate field, and a plan for each private error
//! graph. Each is a function of the phase before it, so each is a tracked query
//! over [`scan_queries::error_apply_file`]: a session computes a phase again only
//! when what it reads changed, and a phase whose result comes out equal stops
//! the recomputation there. Nothing is written into the IR.

use std::path::PathBuf;

use tracing::instrument;

use super::queries::SemanticDatabase;
use super::scan_queries::{self, ScanKey};
use crate::config::ErrorFieldStrategy;
use crate::etiquettes::internal_error_chain::{
    CrateTypeIndex, ErrorConversionUnit, ErrorGraphPlan, graphs, type_index, units,
};
use crate::ir::SourceErrorApply;
use crate::loader::SourceEditView;

/// A crate and the files apply reads of it. A change to the list is a new key:
/// the per-file results beneath it are still reused.
#[salsa::interned]
pub(super) struct ApplyKey<'db> {
    #[returns(ref)]
    root: PathBuf,
    #[returns(ref)]
    files: Vec<(ra_ap_vfs::FileId, PathBuf)>,
}

/// The same, planned for one field strategy.
#[salsa::interned]
pub(super) struct GraphKey<'db> {
    #[returns(copy)]
    apply: ApplyKey<'db>,
    #[returns(copy)]
    strategy: ErrorFieldStrategy,
}

/// Every file's apply facts, joined.
#[salsa::tracked(returns(ref))]
#[instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
pub(super) fn apply_source<'db>(
    db: &'db dyn SemanticDatabase,
    key: ApplyKey<'db>,
) -> Option<SourceErrorApply> {
    tracing::debug!(target: "cordial::semantic::queries", function = %key.root(db).display(), query = "apply_source", "semantic query executed");
    let files = key
        .files(db)
        .iter()
        .map(|(file, path)| {
            let scan = ScanKey::new(
                db,
                *file,
                path.clone(),
                PathBuf::new(),
                key.root(db).clone(),
                String::new(),
                false,
            );
            scan_queries::error_apply_file(db, scan).clone()
        })
        .collect::<Option<Vec<_>>>()?;
    Some(SourceErrorApply::from_files(key.root(db), files))
}

/// The index of declarations paths are resolved against.
#[salsa::tracked(returns(ref))]
#[instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
pub(super) fn apply_index<'db>(
    db: &'db dyn SemanticDatabase,
    key: ApplyKey<'db>,
) -> Option<CrateTypeIndex> {
    tracing::debug!(target: "cordial::semantic::queries", function = %key.root(db).display(), query = "apply_index", "semantic query executed");
    type_index(apply_source(db, key).as_ref()?).ok()
}

/// The conversion unit of every candidate field.
#[salsa::tracked(returns(ref))]
#[instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
pub(super) fn apply_units<'db>(
    db: &'db dyn SemanticDatabase,
    key: ApplyKey<'db>,
) -> Option<Vec<ErrorConversionUnit>> {
    tracing::debug!(target: "cordial::semantic::queries", function = %key.root(db).display(), query = "apply_units", "semantic query executed");
    units(
        apply_source(db, key).as_ref()?,
        apply_index(db, key).as_ref()?,
    )
    .ok()
}

/// The plan of every private error graph.
#[salsa::tracked(returns(ref))]
#[instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
pub(super) fn apply_graphs<'db>(
    db: &'db dyn SemanticDatabase,
    key: GraphKey<'db>,
) -> Option<Vec<ErrorGraphPlan>> {
    let apply = key.apply(db);
    tracing::debug!(target: "cordial::semantic::queries", function = %apply.root(db).display(), query = "apply_graphs", "semantic query executed");
    let source = apply_source(db, apply).as_ref()?;
    let index = apply_index(db, apply).as_ref()?;
    let sema = ra_ap_hir::Semantics::new(db.root_database());
    let views = source
        .files()
        .iter()
        .map(|file| {
            let (id, path) = apply
                .files(db)
                .iter()
                .find(|(_, path)| path == file.snapshot().path())?;
            let text = super::projection::source_of(db.root_database(), &sema, *id, path).ok()?;
            Some(SourceEditView::from_text(
                file.snapshot().clone(),
                text.source().to_owned(),
            ))
        })
        .collect::<Option<Vec<_>>>()?;
    Some(graphs(&views, key.strategy(db), index))
}

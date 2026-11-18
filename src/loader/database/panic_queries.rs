//! Panic evidence as queries over the files.
//!
//! A file's scan reads one fact about the whole crate: the functions the Kani
//! proofs reach. Each file contributes its part of the proof graph (a query keyed
//! by the file), the parts are joined (a query over them), the reachable set is
//! derived from the join, and each file's scan is keyed by the file and that set.
//! An edit that changes neither the file's proof graph nor the reachable set
//! re-scans only the edited file.

use std::collections::BTreeSet;

use tracing::instrument;

use super::crate_files::{CrateKey, FileKey, carried};
use super::queries::SemanticDatabase;
use crate::PanicProofGraph;
use crate::ir::{FileCandidates, FileScan, file_candidates, file_proofs};

/// A file's scan, under the functions the proofs reach.
#[salsa::interned(persist)]
pub(super) struct ScanKey<'db> {
    #[returns(copy)]
    pub(super) file: FileKey<'db>,
    #[returns(ref)]
    pub(super) reachable: BTreeSet<String>,
}

/// What one file contributes to the crate's proof graph.
#[salsa::tracked(persist, returns(ref))]
#[instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
pub(super) fn panic_proofs_file<'db>(
    db: &'db dyn SemanticDatabase,
    key: FileKey<'db>,
) -> Option<PanicProofGraph> {
    carried(db, key, "panic_proofs_file", |source| {
        Ok(file_proofs(source))
    })
}

/// The crate's proof graph, joined from the files'.
#[salsa::tracked(returns(ref))]
#[instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
pub(super) fn panic_graph<'db>(
    db: &'db dyn SemanticDatabase,
    key: CrateKey<'db>,
) -> Option<PanicProofGraph> {
    tracing::debug!(target: "cordial::semantic::queries", function = %key.root(db).display(), query = "panic_graph", "semantic query executed");
    let parts = key
        .file_keys(db)
        .into_iter()
        .map(|file| panic_proofs_file(db, file).as_ref())
        .collect::<Option<Vec<_>>>()?;
    Some(PanicProofGraph::merge(parts.into_iter()))
}

/// The functions the proofs reach: the one thing a file's scan reads of the graph.
#[salsa::tracked(returns(ref))]
#[instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
pub(super) fn panic_reachable<'db>(
    db: &'db dyn SemanticDatabase,
    key: CrateKey<'db>,
) -> Option<BTreeSet<String>> {
    tracing::debug!(target: "cordial::semantic::queries", function = %key.root(db).display(), query = "panic_reachable", "semantic query executed");
    Some(panic_graph(db, key).as_ref()?.reachable())
}

/// A file's panic sites before the proofs say which are reported: what its syntax gives, with no
/// reading of the crate's proof graph, so that the syntax is read once for the proofs and the sites.
#[salsa::tracked(persist, returns(ref))]
#[instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
pub(super) fn panic_candidates_file<'db>(
    db: &'db dyn SemanticDatabase,
    key: FileKey<'db>,
) -> Option<FileCandidates> {
    carried(db, key, "panic_candidates_file", |source| {
        file_candidates(key.tree(db), key.root(db), source)
    })
}

/// A file's panic sites, under the functions the proofs reach: its candidates and that set, and no
/// syntax.
#[salsa::tracked(persist, returns(ref))]
#[instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
pub(super) fn panic_scan_file<'db>(
    db: &'db dyn SemanticDatabase,
    key: ScanKey<'db>,
) -> Option<FileScan> {
    let candidates = panic_candidates_file(db, key.file(db)).as_ref()?;
    tracing::debug!(target: "cordial::semantic::queries", function = %key.file(db).path(db).display(), query = "panic_scan_file", "semantic query executed");
    Some(candidates.resolve(key.reachable(db)))
}

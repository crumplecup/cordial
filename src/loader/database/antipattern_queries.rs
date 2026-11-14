//! Antipattern evidence as queries over the files.
//!
//! The walk that finds a file's candidates reads facts about the whole crate: which
//! traits are local, which types are errors, which are constants. Each file
//! contributes its part of those facts (a query keyed by the file), the parts are
//! joined (a query over all the files' parts), and each file's walk is a query keyed
//! by the file and the joined facts. An edit that leaves the joined facts equal
//! re-walks only the edited file; one that changes them re-walks the files the
//! change can reach, as it must.

use tracing::instrument;

use super::crate_files::{CrateKey, FileKey, carried};
use super::queries::SemanticDatabase;
use crate::FileSnapshot;
use crate::etiquettes::antipatterns::{
    AntipatternCandidate, ContractFile, CrateFacts, FileFacts, file_candidates, file_clauses,
};

/// A file's walk, under the crate's joined facts.
#[salsa::interned(persist)]
pub(super) struct WalkKey<'db> {
    #[returns(copy)]
    pub(super) file: FileKey<'db>,
    #[returns(ref)]
    pub(super) facts: CrateFacts,
}

/// A file's contract clauses, for the crate the clauses are read as.
#[salsa::interned(persist)]
pub(super) struct ContractKey<'db> {
    #[returns(copy)]
    pub(super) file: FileKey<'db>,
    #[returns(ref)]
    pub(super) crate_name: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
/// What a file's walk found, with the file's fingerprint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FileSites {
    pub(super) snapshot: FileSnapshot,
    pub(super) sites: Vec<AntipatternCandidate>,
}

/// What one file contributes to the crate's facts.
#[salsa::tracked(persist, returns(ref))]
#[instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
pub(super) fn antipattern_facts_file<'db>(
    db: &'db dyn SemanticDatabase,
    key: FileKey<'db>,
) -> Option<FileFacts> {
    carried(db, key, "antipattern_facts_file", |source| {
        Ok(FileFacts::collect(source))
    })
}

/// The crate's facts, joined from the files' contributions.
#[salsa::tracked(returns(ref))]
#[instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
pub(super) fn antipattern_crate_facts<'db>(
    db: &'db dyn SemanticDatabase,
    key: CrateKey<'db>,
) -> Option<CrateFacts> {
    tracing::debug!(target: "cordial::semantic::queries", function = %key.root(db).display(), query = "antipattern_crate_facts", "semantic query executed");
    let parts = key
        .file_keys(db)
        .into_iter()
        .map(|file| antipattern_facts_file(db, file).as_ref())
        .collect::<Option<Vec<_>>>()?;
    Some(CrateFacts::join(parts.into_iter()))
}

/// A file's candidates under the crate's facts.
#[salsa::tracked(persist, returns(ref))]
#[instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
pub(super) fn antipattern_sites_file<'db>(
    db: &'db dyn SemanticDatabase,
    key: WalkKey<'db>,
) -> Option<FileSites> {
    let file = key.file(db);
    carried(db, file, "antipattern_sites_file", |source| {
        Ok(FileSites {
            snapshot: FileSnapshot::new(source.path().to_path_buf(), source.source_digest()),
            sites: file_candidates(source, file.tree(db), file.root(db), key.facts(db))?,
        })
    })
}

/// A file's contract clauses, or `None` when this policy does not read the file.
#[salsa::tracked(persist, returns(ref))]
#[instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
pub(super) fn antipattern_contracts_file<'db>(
    db: &'db dyn SemanticDatabase,
    key: ContractKey<'db>,
) -> Option<Option<ContractFile>> {
    let file = key.file(db);
    carried(db, file, "antipattern_contracts_file", |source| {
        file_clauses(file.root(db), key.crate_name(db), source)
    })
}

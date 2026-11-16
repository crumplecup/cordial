//! The definition map of a crate, depended on as its fingerprint.
//!
//! Every function reads the definition map of its crate to resolve a name, and the map is made
//! from the text of every file of the crate, so a memo that read it depends on all of them: an edit
//! to one function's body makes the evidence of every function of the crate stale, though the map
//! the edit makes is the one it had. The fingerprint is a persisted query of the map's value, and
//! what read the map is saved as depending on it (`Database::register_cutoff`): after an edit it
//! runs again, finds the map as it was, and does not change, and what read it is still valid.
//!
//! The fingerprint is `ra_ap_hir_def::nameres::stable`: a hash of everything in the map, with each
//! id replaced by what it stands for, written so that a field rust-analyzer adds to the map is a
//! compile error there until it is accounted for. What it could not give a meaning that survives
//! the process it makes volatile, which is safe (it differs in every process, so nothing is reused)
//! and counted.

use ra_ap_hir::attach_db;
use ra_ap_ide_db::base_db::Crate as InputCrate;
use tracing::instrument;

use super::SourceDatabase;
use super::queries::SemanticDatabase;

/// A hash of the definition map of `krate`.
#[salsa::tracked(persist, returns(copy))]
#[instrument(level = "debug", skip(db))]
#[hotpath::measure]
pub(super) fn def_map_fingerprint(db: &dyn SemanticDatabase, krate: InputCrate) -> u64 {
    let root = db.root_database();
    attach_db(root, || {
        let fingerprint = ra_ap_hir_def::nameres::stable::def_map_fingerprint(root, krate);
        if fingerprint.volatile > 0 {
            tracing::debug!(
                target: "cordial::cache",
                volatile = fingerprint.volatile,
                "a definition map has things in it with no meaning that survives the process: \
                 what read it is not reused across an edit"
            );
        }
        fingerprint.hash
    })
}

impl SourceDatabase {
    /// Whether what is saved depends on the definition map's fingerprint.
    pub(super) fn reuses_across_edits(&self) -> bool {
        self.cargo_options
            .as_ref()
            .is_some_and(|options| *options.reuse_across_edits())
    }

    /// Say that the fingerprint stands for the definition map, so that a save depends on it.
    #[instrument(level = "debug", skip(self))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn declare_cutoffs(&self) {
        if !self.reuses_across_edits() {
            return;
        }
        let declared = <dyn salsa::Database>::register_cutoff(
            &self.db,
            "crate_local_def_map",
            "def_map_fingerprint",
        );
        tracing::debug!(target: "cordial::cache", declared, "the definition map's fingerprint declared");
    }

    /// Ask for the fingerprint of every local crate's definition map, as a save needs to find them.
    #[instrument(level = "debug", skip(self))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn ask_fingerprints(&self) {
        if !self.reuses_across_edits() {
            return;
        }
        attach_db(&self.db, || {
            for krate in ra_ap_hir::Crate::all(&self.db) {
                if krate.origin(&self.db).is_local() {
                    let _ = def_map_fingerprint(&self.db, krate.base());
                }
            }
        });
    }

    /// Ask for one fingerprint, so that a restored database has the query set up before a memo
    /// that depends on it is verified.
    pub(super) fn warm_fingerprint(&self) {
        if !self.reuses_across_edits() {
            return;
        }
        attach_db(&self.db, || {
            if let Some(krate) = ra_ap_hir::Crate::all(&self.db)
                .into_iter()
                .find(|krate| krate.origin(&self.db).is_local())
            {
                let _ = def_map_fingerprint(&self.db, krate.base());
            }
        });
    }
}

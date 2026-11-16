//! A check on what a run reused, made by computing it again.
//!
//! With reuse across edits, what is saved about a function depends on a fingerprint of its crate's
//! definition map instead of on every file the map is made from. That is sound if the fingerprint
//! changes whenever anything a function can read of the map does, which the differential battery in
//! `tests/edit_reuse.rs` tests for the edits it thinks of. This is for the ones it does not: when a
//! run has computed anything, it takes a sample of the functions it did *not* compute, computes
//! each again from the sources, and compares. A function whose reused evidence is not what the
//! sources give is a fingerprint that missed something, said as an error and counted.

use ra_ap_hir::attach_db;
use tracing::instrument;

use super::SourceDatabase;
use super::item_key::{self, ItemKey};
use super::prime::crate_functions;
use super::queries::{self, FunctionKey, function_evidence, persisted_evidence};
use ra_ap_hir::Crate;

/// How many reused functions are computed again, at most.
const SAMPLE: usize = 256;

impl SourceDatabase {
    /// Compute again a sample of the functions whose evidence this run took from the saved state,
    /// when this run has computed any, and say what differs.
    #[instrument(level = "debug", skip(self))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn audit_reused_evidence(&self) {
        if !self.reuses_across_edits() {
            return;
        }
        // The warm-up's own query is computed by every restored run: more than that is an edit.
        let computed = queries::computed_count();
        if computed <= 1 {
            tracing::debug!(
                target: "cordial::audit",
                computed,
                "nothing was computed again, so nothing reused needs checking"
            );
            return;
        }
        attach_db(&self.db, || {
            let mut reused: Vec<String> = Vec::new();
            for krate in self.ordinary_crates() {
                let Some(scope) = item_key::crate_scope(&self.db, Crate::from(krate)) else {
                    continue;
                };
                for text in &crate_functions(&self.db, scope).found {
                    let key = ItemKey::new(&self.db, text.clone());
                    if !queries::computed_this_run(item_key::key_bits(key)) {
                        reused.push(text.clone());
                    }
                }
            }
            let stride = (reused.len() / SAMPLE).max(1);
            let (mut audited, mut mismatched) = (0usize, Vec::new());
            for text in reused.iter().step_by(stride) {
                let key = ItemKey::new(&self.db, text.clone());
                let Some(function) = item_key::function_of(&self.db, key) else {
                    continue;
                };
                audited += 1;
                let saved = persisted_evidence(&self.db, key);
                let fresh = function_evidence(&self.db, FunctionKey::new(&self.db, function));
                if saved != fresh {
                    tracing::error!(
                        target: "cordial::audit",
                        description = %text,
                        "reused evidence is not what the sources give: the definition map's fingerprint missed something"
                    );
                    mismatched.push(text.clone());
                }
            }
            tracing::info!(
                target: "cordial::audit",
                computed,
                reused = reused.len(),
                audited,
                mismatched = mismatched.len(),
                "computed again a sample of what this run reused, and compared"
            );
            crate::host::event(format!(
                "audit: {audited} of {} reused functions computed again, {} differ",
                reused.len(),
                mismatched.len()
            ));
        });
    }
}

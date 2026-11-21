//! Harvesting cfg contexts from copies of the workspace's crates.
//!
//! rust-analyzer resolves only the code a crate's cfg set activates. A context
//! adds copied local crates with its atoms enabled and disabled, preserving the
//! ordinary build's inputs and memoized results. Unchanged files may alias the
//! originals; copied files share source text but have separate file identities
//! and syntax trees. Context-specific semantic results are copied into the IR.
//!
//! Harvesting, observability and follow-up gate discovery must finish before
//! the pass retires values keyed by copied crates and tracked outputs of their
//! queries, including definition maps and module-keyed values. Other eviction
//! follows the run's pressure policy; interned/composite keys and copied inputs
//! can still accumulate.

mod copy_functions;
mod flips;
mod gates;
#[cfg(feature = "tracing")]
mod leaves;
mod merge;
mod pass;
mod plan;
mod reuse;

pub(super) use pass::{ContextsState, FunctionNodes, FunctionNodesState};
pub(super) use plan::guard;

use crate::SemanticContextKey;
use ra_ap_cfg::{CfgAtom, CfgDiff, CfgOptions};
use ra_ap_hir::{Crate, attach_db};
use ra_ap_hir_expand::intern::Symbol;
use ra_ap_ide_db::{RootDatabase, base_db::Crate as InputCrate};
use tracing::instrument;

impl super::SourceDatabase {
    /// Finish the context's persistence work before asking for eviction. Keeping its
    /// fingerprints now lets the final save reuse them without rebuilding its indexes.
    #[instrument(level = "debug", skip(self, context))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    fn finish_context(&mut self, context: &crate::ContextCrates, after: &str) {
        if self.saved.is_some() && !crate::result_store::active() && self.reuses_across_edits() {
            ra_ap_hir::attach_db(&self.db, || {
                for krate in context.shadows() {
                    self.ask_crate_fingerprints(krate);
                }
            });
        }
        let inputs: Vec<_> = context
            .shadows()
            .map(|krate| salsa::plumbing::AsId::as_id(&krate))
            .collect();
        self.retire_context_values(&inputs, after);
        self.evict_regenerable(after);
    }
}

/// The cfg atom named by `text`: a flag, or `key="value"`.
#[instrument(level = "trace")]
pub(super) fn atom(text: &str) -> CfgAtom {
    match text.split_once('=') {
        Some((key, value)) => CfgAtom::KeyValue {
            key: Symbol::intern(key.trim()),
            value: Symbol::intern(value.trim().trim_matches('"')),
        },
        None => CfgAtom::Flag(Symbol::intern(text.trim())),
    }
}

/// The spelling of `atom` that context keys use: `kani` or `feature="alternate"`.
#[instrument(level = "trace", skip(atom))]
pub(super) fn spell(atom: &CfgAtom) -> String {
    match atom {
        CfgAtom::Flag(name) => name.as_str().to_owned(),
        CfgAtom::KeyValue { key, value } => format!("{}=\"{}\"", key.as_str(), value.as_str()),
    }
}

/// The ordinary cfg sets of the workspace's own crates, kept so every diff is
/// applied to the same base without changing the ordinary build.
#[derive(Debug)]
pub(super) struct LocalCfgs {
    ordinary: Vec<(InputCrate, CfgOptions)>,
}

impl LocalCfgs {
    /// Record the ordinary cfg set of every local crate, leaving out the crates a
    /// context added: they hold their own cfg and are never toggled.
    #[instrument(level = "debug", skip(db, excluded))]
    #[hotpath::measure(impl_type = "LocalCfgs")]
    pub(super) fn capture_excluding(
        db: &RootDatabase,
        excluded: &std::collections::BTreeSet<InputCrate>,
    ) -> Self {
        let ordinary = attach_db(db, || {
            Crate::all(db)
                .into_iter()
                .filter(|krate| krate.origin(db).is_local() && !excluded.contains(&krate.base()))
                .map(|krate| (krate.base(), krate.cfg(db).clone()))
                .collect()
        });
        Self { ordinary }
    }

    /// The crates this captured, with their ordinary cfg sets.
    #[instrument(level = "trace", skip(self))]
    pub(super) fn cfgs(&self) -> impl Iterator<Item = (&InputCrate, &CfgOptions)> {
        self.ordinary
            .iter()
            .map(|(krate, options)| (krate, options))
    }

    /// The change to a cfg set that `context` makes.
    #[instrument(level = "trace", skip(context))]
    pub(super) fn diff_of(context: &SemanticContextKey) -> CfgDiff {
        Self::diff(context)
    }

    /// Each local crate's ordinary cfg set beside the one `context` gives it,
    /// computed without touching the database.
    #[instrument(level = "trace", skip(self, context))]
    #[hotpath::measure(impl_type = "LocalCfgs")]
    pub(super) fn pairs(&self, context: &SemanticContextKey) -> Vec<(CfgOptions, CfgOptions)> {
        let diff = Self::diff(context);
        self.ordinary
            .iter()
            .map(|(_, ordinary)| {
                let mut changed = ordinary.clone();
                changed.apply_diff(diff.clone());
                (ordinary.clone(), changed)
            })
            .collect()
    }

    #[instrument(level = "debug", skip(context))]
    #[hotpath::measure]
    fn diff(context: &SemanticContextKey) -> CfgDiff {
        CfgDiff::new(
            context.enabled().iter().map(|text| atom(text)).collect(),
            context.disabled().iter().map(|text| atom(text)).collect(),
        )
    }
}

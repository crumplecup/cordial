//! Cfg contexts as diffs on the database's cfg inputs.
//!
//! rust-analyzer resolves only the code a crate's cfg set activates. A context
//! is a change to that input: the workspace crates' `cfg_options` get the
//! context's atoms enabled and disabled, salsa recomputes what depends on them
//! (the crates' definition maps and whatever reads those), and the facts of the
//! newly active code are copied into the IR before the next diff replaces them.
//! Parse results, item trees, the sysroot and unchanged macro expansions are
//! never recomputed, and only one variant is resident at a time.

mod flips;
mod merge;
mod pass;
mod plan;

pub(super) use pass::FunctionNodes;

use crate::SemanticContextKey;
use ra_ap_cfg::{CfgAtom, CfgDiff, CfgOptions};
use ra_ap_hir::{Crate, attach_db};
use ra_ap_hir_expand::intern::Symbol;
use ra_ap_ide_db::{RootDatabase, base_db::Crate as InputCrate};
use salsa::{Durability, Setter};
use tracing::instrument;

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
/// applied to the same base and the ordinary build can be restored exactly.
#[derive(Debug)]
pub(super) struct LocalCfgs {
    ordinary: Vec<(InputCrate, CfgOptions)>,
}

impl LocalCfgs {
    /// Record the ordinary cfg set of every local crate.
    #[instrument(level = "debug", skip(db))]
    pub(super) fn capture(db: &RootDatabase) -> Self {
        let ordinary = attach_db(db, || {
            Crate::all(db)
                .into_iter()
                .filter(|krate| krate.origin(db).is_local())
                .map(|krate| (krate.base(), krate.cfg(db).clone()))
                .collect()
        });
        Self { ordinary }
    }

    /// Make `context` the active cfg set of every local crate.
    #[instrument(level = "debug", skip(self, db, context), fields(crates = self.ordinary.len()))]
    #[hotpath::measure(impl_type = "LocalCfgs")]
    pub(super) fn enter(&self, db: &mut RootDatabase, context: &SemanticContextKey) {
        let diff = Self::diff(context);
        for (krate, ordinary) in &self.ordinary {
            let mut options = ordinary.clone();
            options.apply_diff(diff.clone());
            Self::set(db, *krate, options);
        }
    }

    /// Each local crate's ordinary cfg set beside the one `context` gives it,
    /// computed without touching the database.
    #[instrument(level = "trace", skip(self, context))]
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
    fn diff(context: &SemanticContextKey) -> CfgDiff {
        CfgDiff::new(
            context.enabled().iter().map(|text| atom(text)).collect(),
            context.disabled().iter().map(|text| atom(text)).collect(),
        )
    }

    /// Return every local crate to its ordinary cfg set.
    #[instrument(level = "debug", skip(self, db))]
    pub(super) fn restore(&self, db: &mut RootDatabase) {
        for (krate, ordinary) in &self.ordinary {
            Self::set(db, *krate, ordinary.clone());
        }
    }

    #[instrument(level = "trace", skip(db, krate, options))]
    fn set(db: &mut RootDatabase, krate: InputCrate, options: CfgOptions) {
        // Same durability the workspace loader gave the input, so the change does
        // not reclassify what depends on it.
        if krate.cfg_options(db) != &options {
            krate
                .set_cfg_options(db)
                .with_durability(Durability::MEDIUM)
                .to(options);
        }
    }
}

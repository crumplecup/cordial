//! Cfg contexts as diffs on the database's cfg inputs.
//!
//! rust-analyzer resolves only the code a crate's cfg set activates. A context
//! is a change to that input: the workspace crates' `cfg_options` get the
//! context's atoms enabled and disabled, salsa recomputes what depends on them
//! (the crates' definition maps and whatever reads those), and the facts of the
//! newly active code are copied into the IR before the next diff replaces them.
//! Parse results, item trees, the sysroot and unchanged macro expansions are
//! never recomputed, and only one variant is resident at a time.

mod copy_functions;
mod flips;
#[cfg(feature = "tracing")]
mod leaves;
mod merge;
mod pass;
mod plan;

pub(super) use pass::FunctionNodes;

use crate::SemanticContextKey;
use ra_ap_cfg::{CfgAtom, CfgDiff, CfgOptions};
use ra_ap_hir::{Crate, attach_db};
use ra_ap_hir_expand::intern::Symbol;
use ra_ap_ide_db::{RootDatabase, base_db::Crate as InputCrate};
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

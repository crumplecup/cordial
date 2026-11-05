//! Located written uses for conservative error-graph closure checks.
//!
//! Collection is independent of the candidate graph. Consumers select tracked
//! names from this required snapshot evidence without traversing syntax again.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

mod compatibility;
mod native;

pub(crate) type LocatedName = (String, usize, usize);

#[derive(Debug, Clone, Serialize, Deserialize, derive_getters::Getters)]
pub(crate) struct ApplyUses {
    source_identifiers: BTreeSet<String>,
    identifiers: BTreeSet<LocatedName>,
    declarations: BTreeSet<LocatedName>,
    paths: Vec<(LocatedName, Vec<String>)>,
    macro_names: BTreeSet<String>,
    #[getter(copy)]
    has_cfg: bool,
}

impl ApplyUses {
    #[tracing::instrument(level = "trace", skip(source))]
    fn empty(source: &str) -> Self {
        Self {
            // Intentionally includes comments and literals: textual mentions
            // are a separate, conservative guard from syntactic uses.
            source_identifiers: source
                .split(|ch: char| !ch.is_alphanumeric() && ch != '_')
                .map(str::to_string)
                .collect(),
            identifiers: BTreeSet::new(),
            declarations: BTreeSet::new(),
            paths: Vec::new(),
            macro_names: BTreeSet::new(),
            has_cfg: false,
        }
    }

    #[tracing::instrument(level = "trace", skip(self))]
    fn normalize(&mut self) {
        // Graph closure tests membership, not traversal order. Qualified-self
        // paths may expose the same prefix through several AST nodes.
        self.paths.sort();
        self.paths.dedup();
    }
}

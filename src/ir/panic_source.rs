//! Snapshot-bound panic and proof-exemption evidence retained by the loader.

use crate::etiquettes::panics::PanicSiteRecord;
use crate::loader::DatabaseSource;
use crate::{CordialResult, FileSnapshot};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

mod assertions;
mod embedded;
mod native;
mod proof;
mod sites;
mod syntax;

/// Required panic inventory on the crate graph root.
pub const ATTR_SOURCE_PANICS: &str = "source_panics";

/// Complete panic inventory, including the source evidence for proof exemptions.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters, derive_new::new,
)]
pub struct SourcePanics {
    crate_root: PathBuf,
    snapshots: Vec<FileSnapshot>,
    proof_graph: PanicProofGraph,
    records: Vec<PanicSiteRecord>,
}

/// Existing Kani exemption contract: a crate-wide graph of bare function names.
/// This preserves the current policy; it does not claim semantic call resolution.
#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    derive_getters::Getters,
    derive_new::new,
)]
pub struct PanicProofGraph {
    calls: BTreeMap<String, Vec<String>>,
    roots: BTreeSet<String>,
}

impl PanicProofGraph {
    /// Compute the fixed-point proof exemption from persisted graph facts.
    #[tracing::instrument(level = "debug", skip(self))]
    pub fn reachable(&self) -> BTreeSet<String> {
        let mut reachable = self.roots.clone();
        let mut frontier: Vec<_> = self.roots.iter().cloned().collect();
        while let Some(key) = frontier.pop() {
            if let Some(calls) = self.calls.get(&key) {
                for call in calls {
                    if reachable.insert(call.clone()) {
                        frontier.push(call.clone());
                    }
                }
            }
        }
        reachable
    }
}

impl SourcePanics {
    #[tracing::instrument(level = "debug", skip(source, verus), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourcePanics")]
    pub(crate) fn collect(
        source: &crate::loader::SourceLoadView,
        root: &std::path::Path,
        #[cfg(feature = "verus_ir")] verus: &crate::VerusCrateIr,
    ) -> CordialResult<Self> {
        crate::etiquettes::panics::collect_panic_inventory(
            root,
            source,
            #[cfg(feature = "verus_ir")]
            Some(verus),
        )
    }
}

#[derive(Clone, Default)]
struct Context {
    modules: Vec<String>,
    implementation: Option<String>,
    functions: Vec<String>,
    cfg_test: bool,
    not_kani: bool,
}

struct Scanner<'a, 'db> {
    source: &'a DatabaseSource<'db>,
    root: &'a Path,
    reachable: &'a BTreeSet<String>,
    embedded: bool,
    assertion_lines: BTreeSet<u32>,
    records: Vec<PanicSiteRecord>,
}

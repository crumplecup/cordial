//! Snapshot-bound panic and proof-exemption evidence retained by the loader.

use crate::FileSnapshot;
use crate::etiquettes::panics::{PanicKind, PanicSiteRecord};
use crate::loader::DatabaseSource;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

mod assertions;
mod embedded;
mod native;
mod proof;
pub(crate) use native::{FileCandidates, FileScan, file_candidates, file_proofs};
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
    /// The crate's graph from each file's: the same names across files are one
    /// function, as the bare-name contract has always had it.
    #[tracing::instrument(level = "debug", skip(parts))]
    #[hotpath::measure]
    pub(crate) fn merge<'a>(parts: impl Iterator<Item = &'a Self>) -> Self {
        let mut calls: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut roots = BTreeSet::new();
        for part in parts {
            for (name, callees) in &part.calls {
                calls
                    .entry(name.clone())
                    .or_default()
                    .extend(callees.iter().cloned());
            }
            roots.extend(part.roots.iter().cloned());
        }
        for callees in calls.values_mut() {
            callees.sort();
            callees.dedup();
        }
        Self { calls, roots }
    }

    /// Compute the fixed-point proof exemption from persisted graph facts.
    #[tracing::instrument(level = "debug", skip(self))]
    #[hotpath::measure]
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
    embedded: bool,
    assertion_lines: BTreeSet<u32>,
    candidates: Vec<PanicCandidate>,
}

/// A panic site before the crate's proofs say whether it is reported.
///
/// A file's scan does not read the crate's proof graph while it walks the syntax: it keeps each site
/// with the function it is in, and what the graph says of that function is applied afterwards, to the
/// candidates alone (see [`PanicCandidate::resolve`]), so the file's syntax is read once.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum PanicCandidate {
    /// Reported unless a proof reaches `function`, the innermost function the site is in (none in
    /// code a proof harness gates, or outside any function); two of one kind on a line are one site.
    Site {
        record: PanicSiteRecord,
        function: Option<String>,
    },
    /// Reported as it is: what an embedded program's own proofs have already settled.
    Settled(PanicSiteRecord),
    /// What a scan of recovered syntax found, settled together as it was found.
    Nested(Vec<PanicCandidate>),
}

impl PanicCandidate {
    /// The records the candidates make under the functions the crate's proofs reach.
    #[tracing::instrument(level = "debug", skip_all)]
    #[hotpath::measure]
    pub(crate) fn resolve(
        candidates: &[Self],
        reachable: &BTreeSet<String>,
    ) -> Vec<PanicSiteRecord> {
        let mut records: Vec<PanicSiteRecord> = Vec::new();
        for candidate in candidates {
            match candidate {
                Self::Site { record, function } => {
                    if function
                        .as_ref()
                        .is_some_and(|name| reachable.contains(name))
                    {
                        continue;
                    }
                    if record.kind() == PanicKind::Unwrap
                        && records.last().is_some_and(|last| {
                            last.kind() == record.kind()
                                && last.line() == record.line()
                                && last.file() == record.file()
                        })
                    {
                        continue;
                    }
                    records.push(record.clone());
                }
                Self::Settled(record) => records.push(record.clone()),
                Self::Nested(inner) => records.extend(Self::resolve(inner, reachable)),
            }
        }
        records
    }
}

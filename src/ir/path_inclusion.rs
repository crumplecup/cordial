//! Shared workspace source, proof-call, module-splice and Cargo evidence.

use crate::FileSnapshot;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use tracing::instrument;

/// Cargo member identity and declared dependencies, captured by the loader.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_new::new, derive_getters::Getters,
)]
pub struct PathInclusionMember {
    name: String,
    root: PathBuf,
    dependencies: BTreeSet<String>,
}

/// Exact snapshot, module targets and proof calls collected from its syntax.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_new::new, derive_getters::Getters,
)]
pub struct PathInclusionSource {
    crate_name: String,
    snapshot: FileSnapshot,
    targets: Vec<String>,
    #[cfg(feature = "tracing")]
    proof_calls: Vec<super::SourceProofFunction>,
}

/// Workspace-wide splice graph and dependencies, with their collected evidence.
///
/// A file spliced into another crate must satisfy both dependency universes.
/// Derive recommendations and tracing verifier policy consult this shared graph
/// instead of rediscovering members, dependencies, or module declarations.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct PathInclusionFacts {
    #[getter(skip)]
    included_by: BTreeMap<PathBuf, Vec<String>>,
    #[getter(skip)]
    crate_dependencies: BTreeMap<String, BTreeSet<String>>,
    #[getter(skip)]
    crate_name_by_root: BTreeMap<PathBuf, String>,
    members: Vec<PathInclusionMember>,
    sources: Vec<PathInclusionSource>,
}

impl PathInclusionFacts {
    /// Build deterministic lookup indexes from loader-owned workspace evidence.
    #[instrument(level = "debug", skip(members, sources))]
    pub(crate) fn from_evidence(
        mut members: Vec<PathInclusionMember>,
        mut sources: Vec<PathInclusionSource>,
    ) -> Self {
        members.sort_by(|a, b| a.name().cmp(b.name()));
        sources.sort_by(|a, b| {
            a.crate_name()
                .cmp(b.crate_name())
                .then(a.snapshot().path().cmp(b.snapshot().path()))
        });
        let crate_dependencies = members
            .iter()
            .map(|member| (member.name().clone(), member.dependencies().clone()))
            .collect();
        let crate_name_by_root = members
            .iter()
            .map(|member| (canonical_or(member.root()), member.name().clone()))
            .collect();
        let mut included_by: BTreeMap<PathBuf, Vec<String>> = BTreeMap::new();
        for source in &sources {
            if let Some(parent) = source.snapshot().path().parent() {
                for target in source.targets() {
                    included_by
                        .entry(canonical_or(&parent.join(target)))
                        .or_default()
                        .push(source.crate_name().clone());
                }
            }
        }
        for consumers in included_by.values_mut() {
            consumers.sort();
            consumers.dedup();
        }
        Self {
            included_by,
            crate_dependencies,
            crate_name_by_root,
            members,
            sources,
        }
    }

    /// Name of a crate that splices `file` in via `#[path]` but lacks
    /// `needed_dep`, if any -- excluding whichever crate natively owns
    /// `owning_crate_root` (splicing a file into the crate that already
    /// natively contains it isn't a real conflict).
    #[instrument(level = "debug", skip(self, file))]
    pub fn blocking_consumer(
        &self,
        file: &Path,
        owning_crate_root: &Path,
        needed_dep: &str,
    ) -> Option<&str> {
        let file = canonical_or(file);
        let owning_name = self
            .crate_name_by_root
            .get(&canonical_or(owning_crate_root));
        let consumers = self.included_by.get(&file)?;
        let needed_dep = normalize(needed_dep);
        consumers
            .iter()
            .find(|consumer| {
                Some(consumer.as_str()) != owning_name.map(String::as_str)
                    && !self
                        .crate_dependencies
                        .get(consumer.as_str())
                        .is_some_and(|deps| deps.contains(&needed_dep))
            })
            .map(String::as_str)
    }

    /// Every crate that splices `file` in via `#[path]`, excluding
    /// whichever crate natively owns it -- the set of *other*
    /// compilation units this file's real content also compiles under.
    #[instrument(level = "debug", skip(self, file))]
    pub fn splice_consumers(&self, file: &Path, owning_crate_root: &Path) -> Vec<&str> {
        let file = canonical_or(file);
        let owning_name = self
            .crate_name_by_root
            .get(&canonical_or(owning_crate_root));
        let Some(consumers) = self.included_by.get(&file) else {
            return Vec::new();
        };
        consumers
            .iter()
            .filter(|consumer| Some(consumer.as_str()) != owning_name.map(String::as_str))
            .map(String::as_str)
            .collect()
    }

    /// Every crate that depends on `crate_name`, directly or
    /// transitively (a real Cargo dependency edge, not a `#[path]`
    /// splice) -- the set of crates whose own compilation pulls
    /// `crate_name`'s source into the same build, and so under the same
    /// compiler flags (e.g. `cargo kani`'s `--cfg kani`, set for the
    /// *whole* dependency graph it compiles, not just the top-level
    /// target crate).
    #[instrument(level = "trace", skip(self))]
    pub fn transitive_dependents(&self, crate_name: &str) -> HashSet<String> {
        let mut dependents = HashSet::new();
        let mut frontier = vec![normalize(crate_name)];
        while let Some(current) = frontier.pop() {
            for (candidate, deps) in &self.crate_dependencies {
                if deps.contains(&current) && dependents.insert(candidate.clone()) {
                    frontier.push(normalize(candidate));
                }
            }
        }
        dependents
    }

    /// Every workspace member's name and crate root, as already
    /// resolved via `cargo_metadata` -- lets a caller that needs to walk
    /// every crate's own source (e.g. a workspace-wide call graph) reuse
    /// this struct's own crate discovery instead of invoking
    /// `cargo_metadata` a second time.
    #[instrument(level = "trace", skip(self))]
    pub fn crate_roots(&self) -> impl Iterator<Item = (&str, &Path)> {
        self.crate_name_by_root
            .iter()
            .map(|(root, name)| (name.as_str(), root.as_path()))
    }
}

#[instrument(level = "trace")]
pub(crate) fn normalize(name: &str) -> String {
    name.replace('-', "_")
}

#[instrument(level = "debug", skip(path))]
pub(crate) fn canonical_or(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

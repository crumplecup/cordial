//! The stages of a projection that a run in processes hands on.
//!
//! A process that ends between stages leaves, in the run's store, what the next needs to go on: the
//! graph as it is, the outline of where its functions are, and where the pass over the cfg contexts
//! has got to. The next process loads the workspace as any does, and does what is left.

use super::declarations::Outline;
use crate::loader::database::contexts::ContextsState;
use crate::loader::database::{SourceDatabase, item_key};
use crate::{CordialResult, CrateIr};
use tracing::instrument;

/// The kind the stages' entries are of in the store.
const KIND: &str = "stage";
const GRAPH: &str = "graph";
const OUTLINE: &str = "outline";
const CONTEXTS: &str = "contexts";
const IDS: &str = "ids";

impl SourceDatabase {
    /// Open the store of this run's inputs, if the run keeps its results in one: the file is named
    /// for what the results depend on, so that a run whose inputs are the same finds them (and a
    /// process of a run that was ended finds what the earlier ones did), and one whose inputs are
    /// not starts with none.
    #[instrument(level = "debug", skip(self), err(level = "warn"))]
    pub(in crate::loader::database) fn open_results_store(&self) -> CordialResult<()> {
        let Some(directory) = crate::result_store::directory_from_environment() else {
            return Ok(());
        };
        let digest = self.results_digest()?;
        let path = directory.join(format!("{digest}.redb"));
        let store = crate::ResultStore::open(&path)?;
        tracing::info!(
            target: "cordial::results",
            digest = %digest,
            path = %path.display(),
            results = store.count(),
            "the results of this run are kept in the store of its inputs"
        );
        crate::install_result_store(Some(std::sync::Arc::new(store)));
        Ok(())
    }

    /// What the results of a run depend on, as a hash: the build of cordial, the Cargo context of the
    /// workspace, how this run was asked to analyse it, and what is in every source file of the
    /// workspace. A result kept under another hash is of other inputs and is not used.
    #[instrument(level = "debug", skip(self), err(level = "warn"))]
    fn results_digest(&self) -> CordialResult<String> {
        use sha2::{Digest, Sha256};
        let hex = |bytes: &[u8]| {
            bytes
                .iter()
                .fold(String::new(), |hex, byte| format!("{hex}{byte:02x}"))
        };
        let context = self.projection_context()?;
        let executable = Sha256::digest(crate::run_cache::executable_fingerprint().as_bytes());
        // Not the context's own identity, which is a hash of the whole environment of the process,
        // and so of the variables the driver of a run in processes sets for each of them: what is
        // hashed is what the analysis depends on, and of the environment what Cargo and the compiler
        // read.
        let environment: std::collections::BTreeMap<String, String> = std::env::vars()
            .filter(|(key, _)| key.starts_with("CARGO") || key.starts_with("RUST"))
            .collect();
        let identity = Sha256::digest(serde_json::to_vec(&(
            context.root(),
            context.features(),
            context.target(),
            context.all_targets(),
            context.test_cfg(),
            context.toolchain(),
            context.sysroot(),
            context.configuration_inputs(),
            context.compiled_inputs(),
            environment,
        ))?);
        let options = Sha256::digest(format!("{:?}", self.cargo_options).as_bytes());
        // Every Rust source file under the workspace's packages, the tests and examples and build
        // scripts with the rest: a package's own list of its entries is of its targets' roots, and an
        // edit to a file that is not one would not change what the results are of.
        let mut sources: Vec<std::path::PathBuf> = Vec::new();
        for package in context.packages() {
            let walk = walkdir::WalkDir::new(package.root())
                .into_iter()
                .filter_entry(|entry| {
                    entry.depth() == 0
                        || !(entry.file_name() == "target"
                            || entry.file_name().to_string_lossy().starts_with('.'))
                });
            sources.extend(
                walk.flatten()
                    .filter(|entry| {
                        entry.file_type().is_file()
                            && entry
                                .path()
                                .extension()
                                .is_some_and(|extension| extension == "rs")
                    })
                    .map(walkdir::DirEntry::into_path),
            );
        }
        sources.sort();
        sources.dedup();
        let mut texts = Sha256::new();
        for path in &sources {
            texts.update(path.to_string_lossy().as_bytes());
            texts.update([0]);
            // What is on the disk, which is what the analysis was loaded from.
            if let Ok(bytes) = std::fs::read(path) {
                texts.update(&bytes);
            }
            texts.update([0]);
        }
        let texts = texts.finalize();
        let mut whole = Sha256::new();
        for part in [&executable[..], &identity[..], &options[..], &texts[..]] {
            whole.update(part);
        }
        let digest = hex(&whole.finalize());
        // Said part by part: a digest that is not the same from one process of a run to the next is
        // one of these that is not, and which it is shows here.
        tracing::debug!(
            target: "cordial::results",
            digest = %digest,
            executable = %hex(&executable[..6]),
            identity = %hex(&identity[..6]),
            options = %hex(&options[..6]),
            sources = sources.len(),
            texts = %hex(&texts[..6]),
            "what the results of this run depend on"
        );
        Ok(digest)
    }

    /// The whole of the projection is done: keep the graph, so that a run with the same inputs
    /// has it without doing any of it again. The process does not end here.
    #[instrument(level = "debug", skip(self, graph))]
    pub(in crate::loader::database) fn stage_complete(&self, graph: &CrateIr) {
        self.stage_kept(graph, None, "complete", false);
    }

    /// Keep the outline, which the passes after the ordinary one read and do not change.
    #[instrument(level = "debug", skip_all)]
    pub(in crate::loader::database) fn store_outline(&self, outline: &Outline) {
        let Some(store) = crate::result_store::current() else {
            return;
        };
        match serde_json::to_vec(&outline.to_state()) {
            Ok(bytes) => store.put(KIND, OUTLINE, &bytes),
            Err(error) => tracing::warn!(
                target: "cordial::results",
                error = %error,
                "the outline could not be kept in the store"
            ),
        }
    }

    /// A stage is done: keep the graph as it is and where the cfg contexts have got to, and let the
    /// process end if it is one that does between stages (see `stage_boundary`).
    #[instrument(level = "debug", skip(self, graph, contexts))]
    pub(in crate::loader::database) fn stage_done(
        &self,
        graph: &CrateIr,
        contexts: Option<&ContextsState>,
        unit: &str,
    ) {
        self.stage_kept(graph, contexts, unit, true);
    }

    /// [`Self::stage_done`], saying whether the process ends at the boundary it may be.
    fn stage_kept(
        &self,
        graph: &CrateIr,
        contexts: Option<&ContextsState>,
        unit: &str,
        boundary: bool,
    ) {
        let Some(store) = crate::result_store::current() else {
            return;
        };
        // The graph's facts hold the numbers this process gave the definitions they are about, which
        // mean nothing in another: what each describes goes with them.
        let described = graph.semantic_identities().map(|ids| {
            ids.into_iter()
                .map(|id| (id.bits(), item_key::description(&self.db, id)))
                .collect::<std::collections::BTreeMap<u64, String>>()
        });
        let kept = described.and_then(|described| {
            store.put(KIND, IDS, &serde_json::to_vec(&described)?);
            Ok(())
        });
        let kept = kept
            .and_then(|()| graph.to_stage_bytes())
            .and_then(|graph_bytes| {
                store.put(KIND, GRAPH, &graph_bytes);
                if let Some(state) = contexts {
                    store.put(KIND, CONTEXTS, &serde_json::to_vec(state)?);
                }
                // One entry for each stage that is done, so that the run's progress is a count that
                // only goes up.
                store.put(KIND, &format!("done-{unit}"), b"");
                Ok(graph_bytes.len())
            });
        match kept {
            Ok(bytes) => tracing::info!(
                target: "cordial::results",
                unit,
                graph_bytes = bytes,
                "a stage is done and kept in the store"
            ),
            Err(error) => {
                tracing::warn!(
                    target: "cordial::results",
                    unit,
                    error = ?error,
                    "a stage could not be kept in the store"
                );
                return;
            }
        }
        if boundary {
            self.stage_boundary();
        }
    }

    /// The graph and outline an earlier process of this run left, if it left them, and where its
    /// pass over the cfg contexts got to (kept for that pass). `None` when this process starts the
    /// stages.
    #[instrument(level = "debug", skip(self))]
    pub(in crate::loader::database) fn take_up_stage(
        &mut self,
    ) -> CordialResult<Option<(CrateIr, Outline)>> {
        let Some(store) = crate::result_store::current() else {
            return Ok(None);
        };
        let (Some(graph), Some(outline)) = (store.get(KIND, GRAPH), store.get(KIND, OUTLINE))
        else {
            return Ok(None);
        };
        let mut graph = CrateIr::from_stage_bytes(&graph)?;
        if let Some(bytes) = store.get(KIND, IDS) {
            let described: std::collections::BTreeMap<u64, String> =
                serde_json::from_slice(&bytes)?;
            // What each number described, as this process numbers it.
            let here: std::collections::BTreeMap<u64, crate::SemanticItemId> =
                ra_ap_hir::attach_db(&self.db, || {
                    described
                        .into_iter()
                        .map(|(was, text)| {
                            let key = item_key::ItemKey::new(&self.db, text);
                            (was, crate::SemanticItemId::new(item_key::key_bits(key)))
                        })
                        .collect()
                });
            graph.map_semantic_identities(&mut |id| here.get(&id.bits()).copied().unwrap_or(id))?;
        }
        let outline = Outline::from_state(serde_json::from_slice(&outline)?);
        self.resume_contexts = match store.get(KIND, CONTEXTS) {
            Some(bytes) => Some(serde_json::from_slice(&bytes)?),
            None => None,
        };
        tracing::info!(
            target: "cordial::results",
            contexts_analysed = self.resume_contexts.as_ref().map_or(0, ContextsState::analysed),
            "the stages are taken up from an earlier process"
        );
        Ok(Some((graph, outline)))
    }

    /// Whether an earlier process of this run finished `unit`.
    #[must_use]
    pub(in crate::loader::database) fn unit_is_done(&self, unit: &str) -> bool {
        crate::result_store::current()
            .is_some_and(|store| store.get(KIND, &format!("done-{unit}")).is_some())
    }

    /// Keep what a pass that is not a stage of its own (the loop over the leaf files' contexts) needs
    /// to be taken up from, say that `unit` of it is done, and let the process end if it is the kind
    /// that does.
    #[cfg(feature = "tracing")]
    #[instrument(level = "debug", skip(self, bytes))]
    pub(in crate::loader::database) fn keep_progress(&self, name: &str, bytes: &[u8], unit: &str) {
        let Some(store) = crate::result_store::current() else {
            return;
        };
        store.put(KIND, name, bytes);
        store.put(KIND, &format!("done-{unit}"), b"");
        tracing::info!(
            target: "cordial::results",
            unit,
            "progress is kept in the store"
        );
        self.stage_boundary();
    }

    /// The evidence of every function is in the store: a process that did the work of it may end
    /// here, so that the next starts the scans and the ordinary projection with nothing held that
    /// the evidence needed.
    #[instrument(level = "debug", skip(self))]
    pub(in crate::loader::database) fn evidence_done(&self) {
        let Some(store) = crate::result_store::current() else {
            return;
        };
        store.put(KIND, "done-evidence", b"");
        self.stage_boundary();
    }

    /// What the loop over the leaf files' contexts was taken up from, if an earlier process got
    /// through some of it.
    #[cfg(feature = "tracing")]
    #[instrument(level = "debug", skip(self))]
    pub(in crate::loader::database) fn leaves_progress(&self) -> Option<Vec<u8>> {
        crate::result_store::current().and_then(|store| store.get(KIND, "leaves"))
    }
}

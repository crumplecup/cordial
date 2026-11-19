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
        self.stage_boundary();
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

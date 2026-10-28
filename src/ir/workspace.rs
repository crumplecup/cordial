use std::collections::HashMap;

use crate::error::{CordialError, CordialResult};
use crate::ir::{CrateIr, EdgeKind, EdgeWeight, IrView, NodeId};

use tracing::instrument;

mod view;
pub use view::{CrateView, CrateViewMut};
/// Workspace-level IR: one graph per crate plus cross-crate edges.
#[derive(Debug, Default, derive_getters::Getters, derive_setters::Setters)]
#[setters(generate = false, borrow_self)]
pub struct WorkspaceIr {
    /// Crate names in this rollup.
    crates: HashMap<String, CrateIr>,
    /// Edges that connect nodes in different crates.
    cross_crate_edges: Vec<(String, NodeId, String, NodeId, EdgeWeight)>,
    /// Dependency survey facts collected once from the workspace manifests and lockfile.
    #[cfg(feature = "dependency_freshness")]
    #[getter(skip)]
    dependency_freshness_records:
        Option<HashMap<String, Vec<crate::etiquettes::DependencySurveyRecord>>>,
    /// Proof harness evidence shared by coverage enrichers and assessors.
    #[cfg(any(feature = "impl_coverage", feature = "amenable_std"))]
    #[getter(skip)]
    proof_harness: Option<std::sync::Arc<crate::proof_harness::ProofHarness>>,
    /// Parsed Verus macro facts for each crate in this run.
    #[cfg(feature = "verus_ir")]
    #[getter(skip)]
    verus_facts: HashMap<String, std::sync::Arc<crate::VerusCrateIr>>,
    /// Workspace manifest and contract evidence used by the antipattern enricher.
    #[cfg(feature = "antipatterns")]
    #[getter(skip)]
    antipattern_facts:
        Option<std::sync::Arc<crate::etiquettes::antipatterns::AntipatternWorkspaceFacts>>,
    /// Workspace source facts shared by derive and tracing enrichers.
    #[cfg(feature = "derives")]
    #[getter(skip)]
    path_inclusions: Option<std::sync::Arc<crate::PathInclusionFacts>>,
    /// Proof-only call reachability shared by tracing enrichers.
    #[cfg(feature = "tracing")]
    #[getter(skip)]
    call_graph: Option<std::sync::Arc<crate::etiquettes::tracing::CallGraphFacts>>,
    /// Foreign-type → elicitation wrapper coverage built from hub crate IR.
    #[cfg(feature = "impl_coverage")]
    #[getter(skip)]
    #[setters(generate, rename = "set_wrapper_coverage_map", strip_option)]
    wrapper_coverage_map: Option<crate::rustdoc::WrapperCoverageMap>,
    /// Merged std/core/alloc inventory shared by framework coverage hooks.
    #[cfg(feature = "homecoming_std")]
    #[getter(skip)]
    framework_std_inventory: Option<Vec<crate::framework_std::StdInventoryItem>>,
}

impl WorkspaceIr {
    #[cfg(feature = "dependency_freshness")]
    #[instrument(level = "trace", skip(self, records))]
    pub(crate) fn set_dependency_freshness_records(
        &mut self,
        records: HashMap<String, Vec<crate::etiquettes::DependencySurveyRecord>>,
    ) {
        self.dependency_freshness_records = Some(records);
    }

    #[cfg(feature = "dependency_freshness")]
    #[instrument(level = "trace", skip(self))]
    pub(crate) fn dependency_freshness_records(
        &self,
        crate_name: &str,
    ) -> Option<&[crate::etiquettes::DependencySurveyRecord]> {
        self.dependency_freshness_records
            .as_ref()?
            .get(crate_name)
            .map(Vec::as_slice)
    }

    #[cfg(feature = "homecoming_std")]
    #[instrument(level = "trace", skip(self, items))]
    pub(crate) fn set_framework_std_inventory(
        &mut self,
        items: Vec<crate::framework_std::StdInventoryItem>,
    ) {
        self.framework_std_inventory = Some(items);
    }

    #[cfg(feature = "homecoming_std")]
    /// Merged standard-library inventory retained for the current run.
    #[instrument(level = "trace", skip(self))]
    pub fn framework_std_inventory(&self) -> Option<&[crate::framework_std::StdInventoryItem]> {
        self.framework_std_inventory.as_deref()
    }

    #[cfg(any(feature = "impl_coverage", feature = "amenable_std"))]
    #[instrument(level = "trace", skip(self, harness))]
    pub(crate) fn set_proof_harness(&mut self, harness: crate::proof_harness::ProofHarness) {
        self.proof_harness = Some(std::sync::Arc::new(harness));
    }

    #[cfg(any(feature = "impl_coverage", feature = "amenable_std"))]
    #[instrument(level = "trace", skip(self))]
    pub(crate) fn proof_harness(
        &self,
    ) -> Option<std::sync::Arc<crate::proof_harness::ProofHarness>> {
        self.proof_harness.clone()
    }

    #[cfg(all(
        feature = "verus_ir",
        any(
            feature = "panics",
            feature = "proof_patterns",
            feature = "verus_warnings"
        )
    ))]
    #[instrument(level = "trace", skip(self, facts))]
    pub(crate) fn set_verus_facts(&mut self, crate_name: String, facts: crate::VerusCrateIr) {
        self.verus_facts
            .insert(crate_name, std::sync::Arc::new(facts));
    }

    #[cfg(feature = "verus_ir")]
    #[instrument(level = "trace", skip(self))]
    pub(crate) fn verus_facts(
        &self,
        crate_name: &str,
    ) -> Option<std::sync::Arc<crate::VerusCrateIr>> {
        self.verus_facts.get(crate_name).cloned()
    }

    #[cfg(feature = "antipatterns")]
    #[instrument(level = "trace", skip(self, facts))]
    pub(crate) fn set_antipattern_facts(
        &mut self,
        facts: crate::etiquettes::antipatterns::AntipatternWorkspaceFacts,
    ) {
        self.antipattern_facts = Some(std::sync::Arc::new(facts));
    }

    #[cfg(feature = "antipatterns")]
    #[instrument(level = "trace", skip(self))]
    pub(crate) fn antipattern_facts(
        &self,
    ) -> Option<&crate::etiquettes::antipatterns::AntipatternWorkspaceFacts> {
        self.antipattern_facts.as_deref()
    }
    #[cfg(feature = "derives")]
    #[instrument(level = "trace", skip(self, facts))]
    pub(crate) fn set_path_inclusions(&mut self, facts: crate::PathInclusionFacts) {
        self.path_inclusions = Some(std::sync::Arc::new(facts));
    }

    #[cfg(feature = "tracing")]
    #[instrument(level = "trace", skip(self, facts))]
    pub(crate) fn set_call_graph(&mut self, facts: crate::etiquettes::tracing::CallGraphFacts) {
        self.call_graph = Some(std::sync::Arc::new(facts));
    }

    #[cfg(feature = "derives")]
    #[instrument(level = "trace", skip(self))]
    pub(crate) fn path_inclusions(&self) -> Option<&crate::PathInclusionFacts> {
        self.path_inclusions.as_deref()
    }

    #[cfg(feature = "tracing")]
    #[instrument(level = "trace", skip(self))]
    pub(crate) fn call_graph(&self) -> Option<&crate::etiquettes::tracing::CallGraphFacts> {
        self.call_graph.as_deref()
    }
    /// Workspace-level wrapper coverage from the elicitation hub, if recorded.
    #[instrument(level = "trace", skip(self))]
    #[cfg(feature = "impl_coverage")]
    pub fn wrapper_coverage_map(&self) -> Option<&crate::rustdoc::WrapperCoverageMap> {
        self.wrapper_coverage_map.as_ref()
    }

    /// Type-node count from graph IR (replaces session inventory cache).
    #[instrument(level = "trace", skip(self))]
    #[cfg(feature = "rustdoc")]
    pub fn rustdoc_inventory_type_count(&self, crate_name: &str) -> usize {
        self.crate_ir(crate_name)
            .map(crate::ir::count_type_nodes)
            .unwrap_or(0)
    }

    /// Crate version recorded on the crate root node, if any.
    #[instrument(level = "trace", skip(self))]
    pub fn crate_version(&self, crate_name: &str) -> Option<String> {
        self.crate_ir(crate_name)
            .and_then(|ir| ir.node_weight(ir.root()))
            .and_then(|weight| weight.attr("crate_version"))
            .and_then(|value| value.as_str())
            .map(str::to_string)
    }

    /// Insert crate.
    #[instrument(level = "debug", skip(self, crate_ir))]
    pub fn insert_crate(&mut self, crate_ir: CrateIr) -> NodeId {
        let root = crate_ir.root();
        self.crates.insert(crate_ir.crate_name().clone(), crate_ir);
        root
    }

    /// Crate ir.
    #[instrument(level = "trace", skip(self))]
    pub fn crate_ir(&self, crate_name: &str) -> Option<&CrateIr> {
        self.crates.get(crate_name)
    }

    /// Match nodes across the crate graphs retained in this workspace.
    #[instrument(level = "trace", skip(self, query))]
    pub fn nodes_matching(&self, query: &dyn crate::ir::Query) -> Vec<crate::ir::NodeRef<'_>> {
        self.crates
            .values()
            .flat_map(|ir| ir.nodes_matching(query))
            .collect()
    }

    /// Crate ir mut.
    #[instrument(level = "debug", skip(self))]
    pub fn crate_ir_mut(&mut self, crate_name: &str) -> Option<&mut CrateIr> {
        self.crates.get_mut(crate_name)
    }

    #[instrument(level = "debug", skip(self))]
    fn require_crate(&self, crate_name: &str) -> CordialResult<&CrateIr> {
        self.crate_ir(crate_name)
            .ok_or_else(|| CordialError::crate_must_exist(crate_name))
    }

    #[instrument(level = "debug", skip(self))]
    fn require_crate_mut(&mut self, crate_name: &str) -> CordialResult<&mut CrateIr> {
        self.crate_ir_mut(crate_name)
            .ok_or_else(|| CordialError::crate_must_exist(crate_name))
    }

    /// Insert cross crate edge.
    #[instrument(level = "debug", skip(self, from_crate, from, to_crate, to, kind))]
    pub fn insert_cross_crate_edge(
        &mut self,
        from_crate: impl Into<String>,
        from: NodeId,
        to_crate: impl Into<String>,
        to: NodeId,
        kind: EdgeKind,
    ) {
        self.cross_crate_edges.push((
            from_crate.into(),
            from,
            to_crate.into(),
            to,
            EdgeWeight::new(kind),
        ));
    }
}

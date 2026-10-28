use super::WorkspaceIr;
use crate::error::CordialResult;
use crate::ir::{EdgeKind, IrMut, IrView, NodeId};
use tracing::instrument;

/// View over one crate inside a workspace.
#[derive(derive_new::new)]
pub struct CrateView<'a> {
    /// Workspace IR this assessor reads.
    workspace: &'a WorkspaceIr,
    /// Cargo package name.
    crate_name: String,
}

impl IrView for CrateView<'_> {
    #[instrument(level = "trace", skip(self))]
    fn crate_name(&self) -> &str {
        &self.crate_name
    }

    #[cfg(feature = "dependency_freshness")]
    #[instrument(level = "trace", skip(self))]
    fn workspace_dependency_freshness_records(
        &self,
    ) -> Option<&[crate::etiquettes::DependencySurveyRecord]> {
        self.workspace
            .dependency_freshness_records(&self.crate_name)
    }

    #[cfg(feature = "homecoming_std")]
    #[instrument(level = "trace", skip(self))]
    fn workspace_framework_std_inventory(
        &self,
    ) -> Option<&[crate::framework_std::StdInventoryItem]> {
        self.workspace.framework_std_inventory()
    }

    #[cfg(any(feature = "impl_coverage", feature = "amenable_std"))]
    #[instrument(level = "trace", skip(self))]
    fn workspace_proof_harness(
        &self,
    ) -> Option<std::sync::Arc<crate::proof_harness::ProofHarness>> {
        self.workspace.proof_harness()
    }

    #[cfg(feature = "verus_ir")]
    #[instrument(level = "trace", skip(self))]
    fn verus_facts(&self) -> Option<std::sync::Arc<crate::VerusCrateIr>> {
        self.workspace.verus_facts(&self.crate_name)
    }

    #[cfg(feature = "antipatterns")]
    #[instrument(level = "trace", skip(self))]
    fn workspace_antipattern_facts(
        &self,
    ) -> Option<std::sync::Arc<crate::etiquettes::antipatterns::AntipatternWorkspaceFacts>> {
        self.workspace
            .antipattern_facts
            .as_ref()
            .map(std::sync::Arc::clone)
    }

    #[cfg(feature = "derives")]
    #[instrument(level = "trace", skip(self))]
    fn workspace_path_inclusions(&self) -> Option<std::sync::Arc<crate::PathInclusionFacts>> {
        self.workspace
            .path_inclusions
            .as_ref()
            .map(std::sync::Arc::clone)
    }

    #[cfg(feature = "tracing")]
    #[instrument(level = "trace", skip(self))]
    fn workspace_call_graph(
        &self,
    ) -> Option<std::sync::Arc<crate::etiquettes::tracing::CallGraphFacts>> {
        self.workspace
            .call_graph
            .as_ref()
            .map(std::sync::Arc::clone)
    }

    #[instrument(level = "trace", skip(self))]
    fn root(&self) -> CordialResult<NodeId> {
        Ok(self.workspace.require_crate(&self.crate_name)?.root())
    }

    #[instrument(level = "trace", skip(self, id))]
    fn node(&self, id: NodeId) -> Option<crate::ir::NodeRef<'_>> {
        self.workspace.crate_ir(&self.crate_name)?.node(id)
    }

    #[instrument(level = "trace", skip(self, query))]
    fn nodes_matching(&self, query: &dyn crate::ir::Query) -> Vec<crate::ir::NodeRef<'_>> {
        self.workspace
            .crate_ir(&self.crate_name)
            .map(|ir| ir.nodes_matching(query))
            .unwrap_or_default()
    }

    #[instrument(level = "trace", skip(self, query))]
    fn workspace_nodes_matching(
        &self,
        query: &dyn crate::ir::Query,
    ) -> Vec<crate::ir::NodeRef<'_>> {
        self.workspace.nodes_matching(query)
    }

    #[instrument(level = "trace", skip(self, id, kind))]
    fn parents(&self, id: NodeId, kind: EdgeKind) -> Vec<NodeId> {
        self.workspace
            .crate_ir(&self.crate_name)
            .map(|ir| ir.parents(id, kind))
            .unwrap_or_default()
    }

    #[instrument(level = "trace", skip(self, id, kind))]
    fn children(&self, id: NodeId, kind: EdgeKind) -> Vec<NodeId> {
        self.workspace
            .crate_ir(&self.crate_name)
            .map(|ir| ir.children(id, kind))
            .unwrap_or_default()
    }

    #[instrument(level = "trace", skip(self, path))]
    fn node_by_path(&self, path: &str) -> Option<NodeId> {
        self.workspace
            .crate_ir(&self.crate_name)
            .and_then(|ir| ir.node_by_path(path))
    }
}

/// Mutable view over one crate inside a workspace.
#[derive(derive_new::new)]
pub struct CrateViewMut<'a> {
    /// Workspace IR this assessor reads.
    workspace: &'a mut WorkspaceIr,
    /// Cargo package name.
    crate_name: String,
}

impl IrView for CrateViewMut<'_> {
    #[instrument(level = "trace", skip(self))]
    fn crate_name(&self) -> &str {
        &self.crate_name
    }

    #[cfg(feature = "dependency_freshness")]
    #[instrument(level = "trace", skip(self))]
    fn workspace_dependency_freshness_records(
        &self,
    ) -> Option<&[crate::etiquettes::DependencySurveyRecord]> {
        self.workspace
            .dependency_freshness_records(&self.crate_name)
    }

    #[cfg(feature = "homecoming_std")]
    #[instrument(level = "trace", skip(self))]
    fn workspace_framework_std_inventory(
        &self,
    ) -> Option<&[crate::framework_std::StdInventoryItem]> {
        self.workspace.framework_std_inventory()
    }

    #[cfg(any(feature = "impl_coverage", feature = "amenable_std"))]
    #[instrument(level = "trace", skip(self))]
    fn workspace_proof_harness(
        &self,
    ) -> Option<std::sync::Arc<crate::proof_harness::ProofHarness>> {
        self.workspace.proof_harness()
    }

    #[cfg(feature = "verus_ir")]
    #[instrument(level = "trace", skip(self))]
    fn verus_facts(&self) -> Option<std::sync::Arc<crate::VerusCrateIr>> {
        self.workspace.verus_facts(&self.crate_name)
    }

    #[cfg(feature = "antipatterns")]
    #[instrument(level = "trace", skip(self))]
    fn workspace_antipattern_facts(
        &self,
    ) -> Option<std::sync::Arc<crate::etiquettes::antipatterns::AntipatternWorkspaceFacts>> {
        self.workspace
            .antipattern_facts
            .as_ref()
            .map(std::sync::Arc::clone)
    }

    #[cfg(feature = "derives")]
    #[instrument(level = "trace", skip(self))]
    fn workspace_path_inclusions(&self) -> Option<std::sync::Arc<crate::PathInclusionFacts>> {
        self.workspace
            .path_inclusions
            .as_ref()
            .map(std::sync::Arc::clone)
    }

    #[cfg(feature = "tracing")]
    #[instrument(level = "trace", skip(self))]
    fn workspace_call_graph(
        &self,
    ) -> Option<std::sync::Arc<crate::etiquettes::tracing::CallGraphFacts>> {
        self.workspace
            .call_graph
            .as_ref()
            .map(std::sync::Arc::clone)
    }

    #[instrument(level = "trace", skip(self))]
    fn root(&self) -> CordialResult<NodeId> {
        Ok(self.workspace.require_crate(&self.crate_name)?.root())
    }

    #[instrument(level = "trace", skip(self, id))]
    fn node(&self, id: NodeId) -> Option<crate::ir::NodeRef<'_>> {
        self.workspace.crate_ir(&self.crate_name)?.node(id)
    }

    #[instrument(level = "trace", skip(self, query))]
    fn nodes_matching(&self, query: &dyn crate::ir::Query) -> Vec<crate::ir::NodeRef<'_>> {
        self.workspace
            .crate_ir(&self.crate_name)
            .map(|ir| ir.nodes_matching(query))
            .unwrap_or_default()
    }

    #[instrument(level = "trace", skip(self, query))]
    fn workspace_nodes_matching(
        &self,
        query: &dyn crate::ir::Query,
    ) -> Vec<crate::ir::NodeRef<'_>> {
        self.workspace.nodes_matching(query)
    }

    #[instrument(level = "trace", skip(self, id, kind))]
    fn parents(&self, id: NodeId, kind: EdgeKind) -> Vec<NodeId> {
        self.workspace
            .crate_ir(&self.crate_name)
            .map(|ir| ir.parents(id, kind))
            .unwrap_or_default()
    }

    #[instrument(level = "trace", skip(self, id, kind))]
    fn children(&self, id: NodeId, kind: EdgeKind) -> Vec<NodeId> {
        self.workspace
            .crate_ir(&self.crate_name)
            .map(|ir| ir.children(id, kind))
            .unwrap_or_default()
    }

    #[instrument(level = "trace", skip(self, path))]
    fn node_by_path(&self, path: &str) -> Option<NodeId> {
        self.workspace
            .crate_ir(&self.crate_name)
            .and_then(|ir| ir.node_by_path(path))
    }
}

impl IrMut for CrateViewMut<'_> {
    #[instrument(level = "trace", skip(self, weight))]
    fn insert_node(&mut self, weight: crate::ir::NodeWeight) -> CordialResult<NodeId> {
        Ok(self
            .workspace
            .require_crate_mut(&self.crate_name)?
            .insert_node(weight))
    }

    #[instrument(level = "trace", skip(self, from, to, kind))]
    fn insert_edge(&mut self, from: NodeId, to: NodeId, kind: EdgeKind) -> CordialResult<()> {
        self.workspace
            .require_crate_mut(&self.crate_name)?
            .insert_edge(from, to, kind)
    }

    #[instrument(level = "trace", skip(self, node, value), err(level = "warn"))]
    fn set_attr(&mut self, node: NodeId, key: &str, value: serde_json::Value) -> CordialResult<()> {
        self.workspace
            .require_crate_mut(&self.crate_name)?
            .set_attr(node, key, value)
    }

    #[instrument(level = "trace", skip(self))]
    fn rebuild_path_index(&mut self) -> CordialResult<()> {
        self.workspace
            .require_crate_mut(&self.crate_name)?
            .rebuild_path_index();
        Ok(())
    }

    #[instrument(level = "trace", skip(self))]
    #[cfg(feature = "impl_coverage")]
    fn workspace_wrapper_coverage(&self) -> Option<&crate::rustdoc::WrapperCoverageMap> {
        self.workspace.wrapper_coverage_map()
    }
}

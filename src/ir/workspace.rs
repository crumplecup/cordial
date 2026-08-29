use std::collections::HashMap;

use crate::error::{CordialError, CordialResult};
use crate::ir::{CrateIr, EdgeKind, EdgeWeight, IrMut, IrView, NodeId};

use tracing::instrument;
/// Workspace-level IR: one graph per crate plus cross-crate edges.
#[derive(Debug, Default)]
pub struct WorkspaceIr {
    /// Crate names in this rollup.
    pub crates: HashMap<String, CrateIr>,
    /// Edges that connect nodes in different crates.
    pub cross_crate_edges: Vec<(String, NodeId, String, NodeId, EdgeWeight)>,
    /// Foreign-type → elicitation wrapper coverage built from hub crate IR.
    #[cfg(feature = "impl_coverage")]
    pub wrapper_coverage_map: Option<crate::rustdoc::WrapperCoverageMap>,
}

impl WorkspaceIr {
    /// Store workspace-level wrapper coverage from the elicitation hub.
    #[instrument(level = "trace", skip(self, map))]
    #[cfg(feature = "impl_coverage")]
    pub fn set_wrapper_coverage_map(&mut self, map: crate::rustdoc::WrapperCoverageMap) {
        self.wrapper_coverage_map = Some(map);
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
            .and_then(|ir| ir.node_weight(ir.root))
            .and_then(|weight| weight.attr("crate_version"))
            .and_then(|value| value.as_str())
            .map(str::to_string)
    }

    /// Insert crate.
    #[instrument(level = "debug", skip(self, crate_ir))]
    pub fn insert_crate(&mut self, crate_ir: CrateIr) -> NodeId {
        let root = crate_ir.root;
        self.crates.insert(crate_ir.crate_name.clone(), crate_ir);
        root
    }

    /// Crate ir.
    #[instrument(level = "trace", skip(self))]
    pub fn crate_ir(&self, crate_name: &str) -> Option<&CrateIr> {
        self.crates.get(crate_name)
    }

    /// Crate ir mut.
    #[instrument(level = "debug", skip(self))]
    pub fn crate_ir_mut(&mut self, crate_name: &str) -> Option<&mut CrateIr> {
        self.crates.get_mut(crate_name)
    }

    #[instrument(level = "debug", skip(self))]
    fn require_crate(&self, crate_name: &str) -> CordialResult<&CrateIr> {
        self.crate_ir(crate_name)
            .ok_or_else(|| CordialError::invariant(format!("crate `{crate_name}` must exist")))
    }

    #[instrument(level = "debug", skip(self))]
    fn require_crate_mut(&mut self, crate_name: &str) -> CordialResult<&mut CrateIr> {
        self.crate_ir_mut(crate_name)
            .ok_or_else(|| CordialError::invariant(format!("crate `{crate_name}` must exist")))
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

/// View over one crate inside a workspace.
pub struct CrateView<'a> {
    /// Workspace IR this assessor reads.
    pub workspace: &'a WorkspaceIr,
    /// Cargo package name.
    pub crate_name: String,
}

impl IrView for CrateView<'_> {
    fn crate_name(&self) -> &str {
        &self.crate_name
    }

    fn root(&self) -> CordialResult<NodeId> {
        Ok(self.workspace.require_crate(&self.crate_name)?.root)
    }

    fn node(&self, id: NodeId) -> Option<crate::ir::NodeRef<'_>> {
        self.workspace.crate_ir(&self.crate_name)?.node(id)
    }

    fn nodes_matching(&self, query: &dyn crate::ir::Query) -> Vec<crate::ir::NodeRef<'_>> {
        self.workspace
            .crate_ir(&self.crate_name)
            .map(|ir| ir.nodes_matching(query))
            .unwrap_or_default()
    }

    fn parents(&self, id: NodeId, kind: EdgeKind) -> Vec<NodeId> {
        self.workspace
            .crate_ir(&self.crate_name)
            .map(|ir| ir.parents(id, kind))
            .unwrap_or_default()
    }

    fn children(&self, id: NodeId, kind: EdgeKind) -> Vec<NodeId> {
        self.workspace
            .crate_ir(&self.crate_name)
            .map(|ir| ir.children(id, kind))
            .unwrap_or_default()
    }

    fn node_by_path(&self, path: &str) -> Option<NodeId> {
        self.workspace
            .crate_ir(&self.crate_name)
            .and_then(|ir| ir.node_by_path(path))
    }
}

/// Mutable view over one crate inside a workspace.
pub struct CrateViewMut<'a> {
    /// Workspace IR this assessor reads.
    pub workspace: &'a mut WorkspaceIr,
    /// Cargo package name.
    pub crate_name: String,
}

impl IrView for CrateViewMut<'_> {
    #[instrument(level = "trace", skip(self))]
    fn crate_name(&self) -> &str {
        &self.crate_name
    }

    #[instrument(level = "trace", skip(self))]
    fn root(&self) -> CordialResult<NodeId> {
        Ok(self.workspace.require_crate(&self.crate_name)?.root)
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

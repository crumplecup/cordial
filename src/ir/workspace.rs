use std::collections::HashMap;

use crate::error::{CordialError, CordialResult};
use crate::ir::{CrateIr, EdgeKind, EdgeWeight, IrMut, IrView, NodeId};

/// Workspace-level IR: one graph per crate plus cross-crate edges.
#[derive(Debug, Default)]
pub struct WorkspaceIr {
    pub crates: HashMap<String, CrateIr>,
    pub cross_crate_edges: Vec<(String, NodeId, String, NodeId, EdgeWeight)>,
    /// Foreign-type → elicitation wrapper coverage built from hub crate IR.
    #[cfg(feature = "impl_coverage")]
    pub wrapper_coverage_map: Option<crate::rustdoc::WrapperCoverageMap>,
}

impl WorkspaceIr {
    #[cfg(feature = "impl_coverage")]
    pub fn set_wrapper_coverage_map(&mut self, map: crate::rustdoc::WrapperCoverageMap) {
        self.wrapper_coverage_map = Some(map);
    }

    #[cfg(feature = "impl_coverage")]
    pub fn wrapper_coverage_map(&self) -> Option<&crate::rustdoc::WrapperCoverageMap> {
        self.wrapper_coverage_map.as_ref()
    }

    /// Type-node count from graph IR (replaces session inventory cache).
    #[cfg(feature = "rustdoc")]
    pub fn rustdoc_inventory_type_count(&self, crate_name: &str) -> usize {
        self.crate_ir(crate_name)
            .map(crate::ir::count_type_nodes)
            .unwrap_or(0)
    }

    pub fn crate_version(&self, crate_name: &str) -> Option<String> {
        self.crate_ir(crate_name)
            .and_then(|ir| ir.node_weight(ir.root))
            .and_then(|weight| weight.attr("crate_version"))
            .and_then(|value| value.as_str())
            .map(str::to_string)
    }

    pub fn insert_crate(&mut self, crate_ir: CrateIr) -> NodeId {
        let root = crate_ir.root;
        self.crates.insert(crate_ir.crate_name.clone(), crate_ir);
        root
    }

    pub fn crate_ir(&self, crate_name: &str) -> Option<&CrateIr> {
        self.crates.get(crate_name)
    }

    pub fn crate_ir_mut(&mut self, crate_name: &str) -> Option<&mut CrateIr> {
        self.crates.get_mut(crate_name)
    }

    fn require_crate(&self, crate_name: &str) -> CordialResult<&CrateIr> {
        self.crate_ir(crate_name)
            .ok_or_else(|| CordialError::invariant(format!("crate `{crate_name}` must exist")))
    }

    fn require_crate_mut(&mut self, crate_name: &str) -> CordialResult<&mut CrateIr> {
        self.crate_ir_mut(crate_name)
            .ok_or_else(|| CordialError::invariant(format!("crate `{crate_name}` must exist")))
    }

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
    pub workspace: &'a WorkspaceIr,
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
    pub workspace: &'a mut WorkspaceIr,
    pub crate_name: String,
}

impl IrView for CrateViewMut<'_> {
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

impl IrMut for CrateViewMut<'_> {
    fn insert_node(&mut self, weight: crate::ir::NodeWeight) -> CordialResult<NodeId> {
        Ok(self
            .workspace
            .require_crate_mut(&self.crate_name)?
            .insert_node(weight))
    }

    fn insert_edge(&mut self, from: NodeId, to: NodeId, kind: EdgeKind) -> CordialResult<()> {
        self.workspace
            .require_crate_mut(&self.crate_name)?
            .insert_edge(from, to, kind)
    }

    fn set_attr(&mut self, node: NodeId, key: &str, value: serde_json::Value) -> CordialResult<()> {
        self.workspace
            .require_crate_mut(&self.crate_name)?
            .set_attr(node, key, value)
    }

    fn rebuild_path_index(&mut self) -> CordialResult<()> {
        self.workspace
            .require_crate_mut(&self.crate_name)?
            .rebuild_path_index();
        Ok(())
    }

    #[cfg(feature = "impl_coverage")]
    fn workspace_wrapper_coverage(&self) -> Option<&crate::rustdoc::WrapperCoverageMap> {
        self.workspace.wrapper_coverage_map()
    }
}

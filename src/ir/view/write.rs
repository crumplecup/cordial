//! Mutation interface for graph enrichment.

use super::IrView;
use crate::error::CordialResult;
use crate::ir::{EdgeKind, NodeId, NodeWeight};

/// Mutable crate IR view for enrichers.
///
/// Enrichers should add facts and then rebuild affected indexes before later
/// probes rely on lookup by qualified path.
pub trait IrMut: IrView {
    /// Insert a node and return its crate-local id.
    fn insert_node(&mut self, weight: NodeWeight) -> CordialResult<NodeId>;
    /// Insert a directed edge of `kind` between existing nodes.
    fn insert_edge(&mut self, from: NodeId, to: NodeId, kind: EdgeKind) -> CordialResult<()>;
    /// Append a JSON attribute value to a node.
    ///
    /// Attribute lookup returns the latest value for a key, so enrichers may
    /// refine loader facts without mutating historical entries in place.
    fn set_attr(&mut self, node: NodeId, key: &str, value: serde_json::Value) -> CordialResult<()>;
    /// Rebuild the path → node index after structural edits.
    fn rebuild_path_index(&mut self) -> CordialResult<()>;

    /// Workspace-level wrapper coverage from the elicitation hub IR.
    #[cfg(feature = "impl_coverage")]
    fn workspace_wrapper_coverage(&self) -> Option<&crate::rustdoc::WrapperCoverageMap> {
        None
    }
}

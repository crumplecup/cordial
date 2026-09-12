use crate::ir::NodeId;

use tracing::instrument;
/// Stable reference to a node in the IR graph.
pub trait IrAnchor: Send + Sync {
    /// Graph node this finding is attached to.
    fn node_id(&self) -> NodeId;
}

/// Concrete IR anchor for built-in plugins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeAnchor(NodeId);

impl NodeAnchor {
    /// Create an anchor for a graph node.
    pub fn new(node_id: NodeId) -> Self {
        Self(node_id)
    }

    /// Graph node this anchor refers to.
    pub fn node_id(&self) -> NodeId {
        self.0
    }
}

impl IrAnchor for NodeAnchor {
    #[instrument(level = "trace", skip(self))]
    fn node_id(&self) -> NodeId {
        self.node_id()
    }
}

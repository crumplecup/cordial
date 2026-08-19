use crate::ir::NodeId;

use tracing::instrument;
/// Stable reference to a node in the IR graph.
pub trait IrAnchor: Send + Sync {
    fn node_id(&self) -> NodeId;
}

/// Concrete IR anchor for built-in plugins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeAnchor(pub NodeId);

impl IrAnchor for NodeAnchor {
    #[instrument(level = "trace", skip(self))]
    fn node_id(&self) -> NodeId {
        self.0
    }
}

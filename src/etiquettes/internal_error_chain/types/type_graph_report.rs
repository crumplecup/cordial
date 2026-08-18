use super::{InternalErrorNodeClass, InternalErrorNodeClassCounts, InternalErrorTypeNode};

use tracing::instrument;
/// Type graph scan output for one crate.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InternalErrorTypeGraphReport {
    pub crate_name: String,
    pub nodes: Vec<InternalErrorTypeNode>,
}

impl InternalErrorTypeGraphReport {
    #[instrument(level = "debug", skip(self))]
    pub fn class_counts(&self) -> InternalErrorNodeClassCounts {
        let mut counts = InternalErrorNodeClassCounts::default();
        for node in &self.nodes {
            match node.node_class {
                InternalErrorNodeClass::InternalLeaf => counts.internal_leaf += 1,
                InternalErrorNodeClass::InternalLink => counts.internal_link += 1,
                InternalErrorNodeClass::ForeignBridge => counts.foreign_bridge += 1,
                InternalErrorNodeClass::UmbrellaWrapper => counts.umbrella_wrapper += 1,
            }
        }
        counts
    }
}

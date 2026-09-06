use super::InternalErrorNodeClass;

use tracing::instrument;
/// Count type-graph nodes by class.
#[derive(Debug, Clone, PartialEq, Eq, Default, derive_getters::Getters)]
pub struct InternalErrorNodeClassCounts {
    #[getter(copy)]
    internal_leaf: usize,
    #[getter(copy)]
    internal_link: usize,
    #[getter(copy)]
    foreign_bridge: usize,
    #[getter(copy)]
    umbrella_wrapper: usize,
}

impl InternalErrorNodeClassCounts {
    #[instrument(level = "trace", skip(self))]
    pub(super) fn record_node_class(&mut self, node_class: InternalErrorNodeClass) {
        match node_class {
            InternalErrorNodeClass::InternalLeaf => self.internal_leaf += 1,
            InternalErrorNodeClass::InternalLink => self.internal_link += 1,
            InternalErrorNodeClass::ForeignBridge => self.foreign_bridge += 1,
            InternalErrorNodeClass::UmbrellaWrapper => self.umbrella_wrapper += 1,
        }
    }

    #[instrument(level = "trace", skip(self))]
    pub fn internal_links_total(&self) -> usize {
        self.internal_link + self.umbrella_wrapper
    }
}

/// Count type-graph nodes by class.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InternalErrorNodeClassCounts {
    pub internal_leaf: usize,
    pub internal_link: usize,
    pub foreign_bridge: usize,
    pub umbrella_wrapper: usize,
}

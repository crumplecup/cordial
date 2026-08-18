/// Per-crate rollup row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InternalErrorChainCrateSummary {
    pub crate_name: String,
    pub type_nodes: usize,
    pub internal_leaves: usize,
    pub internal_links: usize,
    pub foreign_bridges: usize,
    pub compliance_findings: usize,
    pub stringify_violations: usize,
    pub discard_violations: usize,
}

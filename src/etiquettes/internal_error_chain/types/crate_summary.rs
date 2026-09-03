/// Per-crate rollup row.
#[derive(Debug, Clone, PartialEq, Eq, derive_builder::Builder, derive_getters::Getters)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub struct InternalErrorChainCrateSummary {
    crate_name: String,
    #[getter(copy)]
    type_nodes: usize,
    #[getter(copy)]
    internal_leaves: usize,
    #[getter(copy)]
    internal_links: usize,
    #[getter(copy)]
    foreign_bridges: usize,
    #[getter(copy)]
    compliance_findings: usize,
    #[getter(copy)]
    stringify_violations: usize,
    #[getter(copy)]
    discard_violations: usize,
}

impl InternalErrorChainCrateSummary {
    /// Start a builder for this value.
    pub fn builder() -> InternalErrorChainCrateSummaryBuilder {
        InternalErrorChainCrateSummaryBuilder::default()
    }
}

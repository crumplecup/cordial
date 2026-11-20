//! Requested heap snapshot types. Collection requires `hotpath-retained`.
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "json", serde(rename_all = "snake_case"))]
pub enum OriginKind {
    Unscoped,
    Recorder,
    TlsUnavailable,
    Overflow,
    Function,
}

#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetainedOrigin {
    /// Allocator-instance identity, stable until that allocator is dropped.
    pub id: usize,
    pub kind: OriginKind,
    pub name: Option<String>,
    pub live_bytes: usize,
    pub live_allocations: usize,
    /// Individual origin's lifetime peak; these peaks must not be summed.
    pub peak_live_bytes: usize,
}

/// One coherent registry snapshot. Requested bytes exclude the embedded registry,
/// private block prefixes, allocator overhead/retention, native allocations and
/// temporary realloc backing. Recorder heap allocations are included.
/// All counters and peaks span this allocator's lifetime, with a zero baseline.
/// Origin rows are exclusive, even when function traffic uses inclusive mode.
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeapSnapshot {
    pub live_bytes: usize,
    pub peak_live_bytes: usize,
    pub live_allocations: usize,
    pub allocated_bytes: u64,
    pub freed_bytes: u64,
    pub successful_allocations: u64,
    pub successful_reallocations: u64,
    pub deallocations: u64,
    pub allocation_failures: u64,
    pub reallocation_failures: u64,
    pub origin_capacity: usize,
    pub registered_origins: usize,
    pub overflow_allocations: u64,
    pub overflow_allocated_bytes: u64,
    /// Embedded registry storage, outside requested-byte accounting.
    pub registry_bytes: usize,
    /// Prefixes including alignment padding of currently live user blocks.
    pub live_header_bytes: usize,
    pub origins: Vec<RetainedOrigin>,
}

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

/// Interval used by the simultaneous peak, independent of guard boundaries.
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "json", serde(rename_all = "snake_case"))]
pub enum PeakInterval {
    AllocatorLifetimeZeroBaseline,
}

/// Equal byte totals preserve the composition at the first attainment.
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "json", serde(rename_all = "snake_case"))]
pub enum PeakTiePolicy {
    FirstAttainment,
}

#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeakOrigin {
    /// Identity in `HeapSnapshot::origins`, not a name-based aggregate.
    pub id: usize,
    pub live_bytes: usize,
    pub live_allocations: usize,
}

/// Exclusive origin composition at one linearized allocator event. These rows
/// can be summed; individual origin peaks and ending ownership cannot.
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeapPeak {
    pub schema_version: u32,
    pub interval: PeakInterval,
    pub tie_policy: PeakTiePolicy,
    /// Successful alloc/free/realloc sequence; zero denotes construction.
    /// Failed requests do not advance this sequence. This is not wall time.
    pub event_sequence: u64,
    pub live_bytes: usize,
    pub live_allocations: usize,
    /// Number of changed-origin copies across all peak updates through this peak.
    /// Bounded by the number of successful allocator events, not origins × events.
    pub origin_updates: u64,
    pub origins: Vec<PeakOrigin>,
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
    /// Absent in older captures; never substitute ending ownership.
    #[cfg_attr(feature = "json", serde(default))]
    pub peak: Option<HeapPeak>,
}

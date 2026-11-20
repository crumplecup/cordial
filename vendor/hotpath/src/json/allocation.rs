//! Exact allocation data. All coverage counters span the process lifetime;
//! function rows span the report's collection window and must not be added in
//! inclusive mode. Unsupported/disabled measurements never become zero.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AllocationStatus {
    Available,
    Disabled,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AllocationAccounting {
    Exclusive,
    Inclusive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExactFunctionAllocation {
    pub id: u32,
    pub name: String,
    pub calls: u64,
    pub bytes: Option<u64>,
    pub count: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExactThreadAllocation {
    pub tid: u64,
    pub name: Option<String>,
    pub allocated_bytes: u64,
    pub freed_bytes: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AllocationCoverage {
    pub thread_tracking_started: bool,
    pub thread_capacity: usize,
    pub registered_threads: usize,
    pub overflow_threads: u64,
    pub overflow_allocated_bytes: u64,
    pub overflow_freed_bytes: u64,
    pub hidden_thread_allocated_bytes: u64,
    pub hidden_thread_freed_bytes: u64,
    pub recorder_allocated_bytes: u64,
    pub recorder_freed_bytes: u64,
    pub unscoped_allocated_bytes: u64,
    pub unscoped_allocation_count: u64,
    pub before_thread_tracking_allocated_bytes: u64,
    pub before_thread_tracking_freed_bytes: u64,
    pub tls_unavailable_allocated_bytes: u64,
    pub tls_unavailable_freed_bytes: u64,
    pub depth_limit: usize,
    pub depth_limit_hits: u64,
    pub unwound_polls: u64,
    pub cross_thread_guards: u64,
    pub inactive_measurements: u64,
    pub dropped_measurements: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AllocationReport {
    pub schema_version: u32,
    pub heap_status: AllocationStatus,
    pub heap_baseline: Option<crate::HeapSnapshot>,
    pub heap: Option<crate::HeapSnapshot>,
    pub functions_status: AllocationStatus,
    pub accounting: Option<AllocationAccounting>,
    pub functions: Option<Vec<ExactFunctionAllocation>>,
    pub threads: Option<Vec<ExactThreadAllocation>>,
    pub coverage: Option<AllocationCoverage>,
}

impl AllocationReport {
    pub fn disabled() -> Self {
        Self {
            schema_version: 1,
            heap_status: AllocationStatus::Disabled,
            heap_baseline: None,
            heap: None,
            functions_status: AllocationStatus::Disabled,
            accounting: None,
            functions: None,
            threads: None,
            coverage: None,
        }
    }
}

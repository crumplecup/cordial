//! Import Hotpath's exact quantities with producer identity and explicit availability.
use crate::profiling::{ReportStatus, RunReport};
use hotpath::json::{AllocationAccounting, AllocationReport, AllocationStatus};
use serde::{Deserialize, Serialize};
use std::fmt::Write;
use std::path::Path;

/// Availability of allocation data, distinct from complete operation attribution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryStatus {
    /// Exact fields are available, including explicit zeros.
    Available,
    /// Allocation recording was disabled in the producer.
    Disabled,
    /// The producer/schema cannot supply the requested quantities.
    Unsupported,
    /// Expected capture did not appear.
    Missing,
    /// Invalid JSON or inconsistent exact schema.
    Malformed,
    /// The JSON document ended before completion.
    Truncated,
    /// Capture did not finish or could not publish.
    Incomplete,
}

/// One producer's raw evidence. Process/thread/origin ids are local to this capture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryCapture {
    /// Lifecycle, identity, phases, features and source paths.
    pub capture: RunReport,
    /// Whether exact allocation quantities can be read.
    pub status: MemoryStatus,
    /// Hotpath's own exact schema; no second accounting implementation.
    pub allocation: Option<AllocationReport>,
    /// Coverage losses and unsupported boundaries; an empty list alone is not runtime proof.
    pub issues: Vec<String>,
}

impl MemoryCapture {
    /// Read one collected process without turning unavailable quantities into zeros.
    pub fn read(capture: RunReport) -> Self {
        let status = match capture.status {
            ReportStatus::Complete => MemoryStatus::Available,
            ReportStatus::Disabled => MemoryStatus::Disabled,
            ReportStatus::Missing => MemoryStatus::Missing,
            ReportStatus::Malformed => MemoryStatus::Malformed,
            ReportStatus::Truncated => MemoryStatus::Truncated,
            _ => MemoryStatus::Incomplete,
        };
        let mut result = Self {
            capture,
            status,
            allocation: None,
            issues: Vec::new(),
        };
        if status != MemoryStatus::Available {
            return result;
        }
        let Some(manifest) = &result.capture.manifest else {
            result.status = MemoryStatus::Missing;
            return result;
        };
        if manifest.schema != 1 {
            result.status = MemoryStatus::Unsupported;
            result
                .issues
                .push("unsupported capture manifest schema".into());
            return result;
        }
        if !matches!(
            manifest.completion,
            Some(crate::profiling::FinishReason::Normal | crate::profiling::FinishReason::Error)
        ) {
            result
                .issues
                .push("ordinary workload teardown was not completed".into());
        }
        if manifest
            .finished_ns
            .is_none_or(|finished| finished < manifest.started_ns)
        {
            result
                .issues
                .push("capture time interval unavailable or invalid".into());
        }
        if manifest.identity.boundary == "first_init_tracing" {
            result
                .issues
                .push("legacy test capture begins after test setup".into());
        }
        let report = std::fs::File::open(&manifest.report).map(std::io::BufReader::new);
        let report: serde_json::Value = match report.map(serde_json::from_reader) {
            Ok(Ok(report)) => report,
            Ok(Err(error)) => {
                result.status = if error.is_eof() {
                    MemoryStatus::Truncated
                } else {
                    MemoryStatus::Malformed
                };
                result.issues.push(error.to_string());
                return result;
            }
            Err(error) => {
                result.status = MemoryStatus::Missing;
                result.issues.push(error.to_string());
                return result;
            }
        };
        let Some(raw) = report.get("allocation").filter(|value| !value.is_null()) else {
            result.status = if manifest.allocation_enabled {
                MemoryStatus::Unsupported
            } else {
                MemoryStatus::Disabled
            };
            result.issues.push(
                "exact allocation schema absent; formatted values are approximate legacy data"
                    .into(),
            );
            return result;
        };
        if raw["schema_version"].as_u64() != Some(1) {
            result.status = MemoryStatus::Unsupported;
            result
                .issues
                .push("unsupported Hotpath allocation schema".into());
            return result;
        }
        let allocation: AllocationReport = match serde_json::from_value(raw.clone()) {
            Ok(allocation) => allocation,
            Err(error) => {
                result.status = MemoryStatus::Malformed;
                result.issues.push(error.to_string());
                return result;
            }
        };
        result.status = match allocation.functions_status {
            AllocationStatus::Disabled => MemoryStatus::Disabled,
            AllocationStatus::Unavailable => MemoryStatus::Unsupported,
            AllocationStatus::Available => MemoryStatus::Available,
        };
        if allocation.functions_status == AllocationStatus::Available
            && (allocation.functions.is_none() || allocation.accounting.is_none())
        {
            result.status = MemoryStatus::Malformed;
            result
                .issues
                .push("available function data requires rows and accounting mode".into());
        }
        if allocation.heap_status == AllocationStatus::Available && allocation.heap.is_none() {
            result.status = MemoryStatus::Malformed;
            result
                .issues
                .push("available heap data requires a snapshot".into());
        }
        let accounting = match allocation.accounting {
            Some(AllocationAccounting::Exclusive) => "exclusive",
            Some(AllocationAccounting::Inclusive) => "inclusive",
            None => "disabled",
        };
        if accounting != manifest.accounting
            || (result.status == MemoryStatus::Available && !manifest.allocation_enabled)
        {
            result.status = MemoryStatus::Malformed;
            result
                .issues
                .push("producer mode and raw allocation data disagree".into());
        }
        if let Some(mode) = report["functions_alloc"]["profiling_mode"].as_str() {
            let expected = match manifest.traffic_metric.as_str() {
                "bytes" => "alloc-bytes",
                "count" => "alloc-count",
                _ => "disabled",
            };
            if mode != expected {
                result.status = MemoryStatus::Malformed;
                result
                    .issues
                    .push("display metric disagrees with producer mode".into());
            }
        }
        for section in ["functions_timing", "functions_alloc", "threads"] {
            if let (Some(total), Some(included)) = (
                report[section]["total_count"].as_u64(),
                report[section]["included_count"].as_u64(),
            ) && included < total
            {
                result.issues.push(format!("{section} display truncated: {included}/{total} rows; exact allocation data retained separately"));
            }
        }
        if allocation.functions_status != AllocationStatus::Available
            && allocation.functions.is_some()
        {
            result.status = MemoryStatus::Malformed;
            result
                .issues
                .push("unavailable function data contains rows".into());
        }
        if allocation.heap_status != AllocationStatus::Available && allocation.heap.is_some() {
            result.status = MemoryStatus::Malformed;
            result
                .issues
                .push("unavailable heap data contains a snapshot".into());
        }
        if allocation.threads.is_none() {
            result
                .issues
                .push("exact thread identities unavailable".into());
        }
        if let Some(coverage) = &allocation.coverage {
            for (name, value) in [
                ("overflow threads", coverage.overflow_threads),
                ("overflow freed bytes", coverage.overflow_freed_bytes),
                (
                    "before thread tracking freed bytes",
                    coverage.before_thread_tracking_freed_bytes,
                ),
                (
                    "TLS unavailable freed bytes",
                    coverage.tls_unavailable_freed_bytes,
                ),
                (
                    "hidden thread allocated bytes",
                    coverage.hidden_thread_allocated_bytes,
                ),
                (
                    "hidden thread freed bytes",
                    coverage.hidden_thread_freed_bytes,
                ),
                (
                    "unscoped allocation count",
                    coverage.unscoped_allocation_count,
                ),
                (
                    "overflow allocated bytes",
                    coverage.overflow_allocated_bytes,
                ),
                (
                    "unscoped allocated bytes",
                    coverage.unscoped_allocated_bytes,
                ),
                (
                    "before thread tracking bytes",
                    coverage.before_thread_tracking_allocated_bytes,
                ),
                (
                    "TLS unavailable bytes",
                    coverage.tls_unavailable_allocated_bytes,
                ),
                ("depth limit hits", coverage.depth_limit_hits),
                ("unwound polls", coverage.unwound_polls),
                ("cross-thread guards", coverage.cross_thread_guards),
                ("inactive measurements", coverage.inactive_measurements),
                ("dropped measurements", coverage.dropped_measurements),
            ] {
                if value != 0 {
                    result.issues.push(format!("{name}: {value}"));
                }
            }
            if !coverage.thread_tracking_started {
                result.issues.push("thread tracking did not start".into());
            }
        } else {
            result.issues.push("allocation coverage unavailable".into());
        }
        if allocation.heap_status == AllocationStatus::Available
            && allocation.heap_baseline.is_none()
        {
            result
                .issues
                .push("requested heap baseline unavailable".into());
        }
        if let Some(heap) = &allocation.heap {
            if heap.overflow_allocations != 0 || heap.overflow_allocated_bytes != 0 {
                result.issues.push(format!(
                    "retained origin overflow allocations: {}; bytes: {}",
                    heap.overflow_allocations, heap.overflow_allocated_bytes
                ));
            }
        } else {
            result
                .issues
                .push("retained heap/origins unsupported".into());
        }
        if allocation
            .functions
            .as_ref()
            .is_some_and(|rows| rows.iter().any(|row| row.bytes.is_none()))
        {
            result.issues.push("some operation byte totals unavailable (async navigation rows); poll scopes remain separate".into());
        }
        if manifest.boundaries.is_empty() {
            result
                .issues
                .push("producer coverage inventory unavailable".into());
        }
        if !manifest.overrides.is_empty() {
            result.issues.push(format!(
                "collection/display overrides: {:?}",
                manifest.overrides
            ));
        }
        result.allocation = Some(allocation);
        result
    }

    /// This run can claim complete attribution only with no losses or unverified boundaries.
    pub fn attribution_complete(&self) -> bool {
        self.status == MemoryStatus::Available
            && self.issues.is_empty()
            && self.capture.manifest.as_ref().is_some_and(|manifest| {
                !manifest.boundaries.is_empty()
                    && manifest
                        .boundaries
                        .iter()
                        .all(|row| row.unverified.is_empty())
            })
    }

    /// Stable workload key across runs. Never merge threads or peaks by their names.
    pub fn key(&self) -> String {
        let identity = self.capture.expected.as_ref().or_else(|| {
            self.capture
                .manifest
                .as_ref()
                .map(|manifest| &manifest.identity)
        });
        format!(
            "{:?}/{:?}/{:?}/{:?}/{:?}/restart={}",
            identity.map(|id| &id.role),
            identity.and_then(|id| id.state.as_ref()),
            identity.and_then(|id| id.repeat),
            identity.and_then(|id| id.binary.as_ref()),
            identity.and_then(|id| id.test.as_ref()),
            self.capture.manifest.as_ref().map_or_else(
                || self
                    .capture
                    .observation
                    .as_ref()
                    .map_or(0, |child| child.restart),
                |manifest| manifest.restart
            )
        )
    }
}

/// Shared importer used by workflow children and test profiles.
pub fn read_memory(root: &Path) -> crate::CordialResult<Vec<MemoryCapture>> {
    Ok(super::read_captures(root)?
        .into_iter()
        .map(MemoryCapture::read)
        .collect())
}

/// Render exact values per producer. Peaks and ending heap are never added across captures.
pub fn render_memory(captures: &[MemoryCapture], top: usize) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "\nMemory: {} process captures; full-program attribution unverified.",
        captures.len()
    );
    if captures.is_empty() {
        let _ = writeln!(
            out,
            "  unavailable: no capture inventory (legacy or unprofiled run)"
        );
    }
    for item in captures {
        let _ = writeln!(
            out,
            "  {} {}: {:?}; attribution complete={}",
            item.capture.process_id,
            item.key(),
            item.status,
            item.attribution_complete()
        );
        if let Some(observation) = &item.capture.observation {
            let _ = writeln!(
                out,
                "    observed pid={:?} exited={} code={:?} error={:?}",
                observation.pid, observation.exited, observation.exit_code, observation.error
            );
        }
        if let Some(manifest) = &item.capture.manifest {
            let _ = writeln!(
                out,
                "    pid={} parent={:?} interval={}..{:?} boundary={} completion={:?}",
                manifest.pid,
                manifest.parent,
                manifest.started_ns,
                manifest.finished_ns,
                manifest.identity.boundary,
                manifest.completion
            );
            let _ = writeln!(
                out,
                "    producer={} revision={:?} dirty_at_build={:?} features={:?} traffic={} accounting={} raw={}",
                manifest.hotpath_version,
                manifest.producer_revision,
                manifest.producer_dirty,
                manifest.features,
                manifest.traffic_metric,
                manifest.accounting,
                manifest.report.display()
            );
        }
        if let Some(allocation) = &item.allocation {
            if let Some(heap) = &allocation.heap {
                let _ = writeln!(
                    out,
                    "    requested heap: live={} B; lifetime peak={} B; baseline live={:?} B (peak interval: allocator lifetime, zero baseline)",
                    heap.live_bytes,
                    heap.peak_live_bytes,
                    allocation
                        .heap_baseline
                        .as_ref()
                        .map(|base| base.live_bytes)
                );
                let _ = writeln!(
                    out,
                    "    heap traffic: allocated={} B freed={} B; registry={} B live headers={} B outside requested sizes",
                    heap.allocated_bytes,
                    heap.freed_bytes,
                    heap.registry_bytes,
                    heap.live_header_bytes
                );
                let mut origins: Vec<_> = heap.origins.iter().collect();
                origins.sort_by_key(|origin| std::cmp::Reverse(origin.live_bytes));
                for origin in origins.into_iter().take(top) {
                    let _ = writeln!(
                        out,
                        "      origin {} {:?} {:?}: live={} B allocations={} individual peak={} B",
                        origin.id,
                        origin.kind,
                        origin.name,
                        origin.live_bytes,
                        origin.live_allocations,
                        origin.peak_live_bytes
                    );
                }
            } else {
                let _ = writeln!(
                    out,
                    "    requested heap/origins: {:?}",
                    allocation.heap_status
                );
            }
            let _ = writeln!(
                out,
                "    function traffic: {:?} {:?}; exact bytes and counts are separate; inclusive rows must not be summed",
                allocation.functions_status, allocation.accounting
            );
            if let Some(rows) = &allocation.functions {
                for row in rows.iter().take(top) {
                    let _ = writeln!(
                        out,
                        "      function {} {}: calls={} bytes={:?} count={:?}",
                        row.id, row.name, row.calls, row.bytes, row.count
                    );
                }
            }
            if let Some(rows) = &allocation.threads {
                for row in rows {
                    let _ = writeln!(
                        out,
                        "      thread {} {:?}: allocated={} B freed={} B (freeing thread, not retained ownership)",
                        row.tid, row.name, row.allocated_bytes, row.freed_bytes
                    );
                }
            }
            if let Some(coverage) = &allocation.coverage {
                let _ = writeln!(
                    out,
                    "    Hotpath coverage (allocator lifetime, not only operation intervals): {coverage:?}"
                );
            }
        }
        for issue in &item.issues {
            let _ = writeln!(out, "    gap: {issue}");
        }
    }
    let mut boundaries = std::collections::BTreeMap::new();
    for item in captures {
        if let Some(manifest) = &item.capture.manifest {
            for boundary in &manifest.boundaries {
                boundaries
                    .entry((&boundary.region, &boundary.unverified))
                    .or_insert(&boundary.evidence);
            }
        }
    }
    for ((region, unverified), evidence) in boundaries {
        let _ = writeln!(
            out,
            "  boundary {region}: {evidence}; unverified: {unverified}"
        );
    }
    let _ = writeln!(
        out,
        "  OS envelope: unavailable here; native allocations, mmap, stacks, shared pages and allocator retention are outside requested heap. No RSS-minus-heap ownership inference. Per-process/origin peaks and ending live bytes are not summed."
    );
    out
}

/// Import only a workflow attempt and its linked resumed descendants.
pub(crate) fn read_child_memory(root: &Path, id: &str) -> crate::CordialResult<Vec<MemoryCapture>> {
    let captures = read_memory(root)?;
    let mut ids = std::collections::BTreeSet::from([id.to_owned()]);
    loop {
        let before = ids.len();
        for item in &captures {
            if ids.contains(&item.capture.process_id)
                && let Some(manifest) = &item.capture.manifest
            {
                ids.extend(manifest.children.iter().map(|child| child.id.clone()));
            }
            if item
                .capture
                .manifest
                .as_ref()
                .and_then(|manifest| manifest.parent.as_ref())
                .is_some_and(|parent| ids.contains(parent))
            {
                ids.insert(item.capture.process_id.clone());
            }
        }
        if ids.len() == before {
            break;
        }
    }
    Ok(captures
        .into_iter()
        .filter(|item| ids.contains(&item.capture.process_id))
        .collect())
}

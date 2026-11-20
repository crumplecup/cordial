//! What a `hotpath` profile says besides time: the bytes each function allocated,
//! what each thread did, how long locks were waited for, how channels filled, and
//! the sizes the program reported with `val!`.
//!
//! The report prints these for reading, so they are parsed back: durations by
//! [`parse_duration`](super::parse_duration), byte counts here. A byte figure above
//! a kilobyte carries one decimal of its unit, so it is a trend, not an exact
//! count; the values the program reports with `val!` are exact, and the ones that
//! count things (files, crates, memos) gate.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tracing::instrument;

use super::parse_duration;

/// Names that end a value's key when it counts things exactly.
const EXACT_SUFFIXES: &[&str] = &[".files", ".crates", ".memos", ".count"];

/// What the comparison gates on: counts of what cordial itself stores and copies.
/// The Salsa database's memo counts are recorded too, but most of them are
/// rust-analyzer's own queries and move with its version, so they trend like any
/// other value and do not fail a comparison.
const GATED_PREFIXES: &[&str] = &["storage.store.", "storage.context."];

/// Bytes allocated by one function over a run.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters, derive_new::new,
)]
pub struct FunctionAllocation {
    name: String,
    #[getter(copy)]
    calls: u64,
    #[getter(copy)]
    total_bytes: u64,
}

/// One thread's use of the processor and the heap.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters, derive_new::new,
)]
pub struct ThreadUsage {
    name: String,
    /// The most processor the thread used in any sample, in tenths of a percent
    /// (a thread running flat out reads about 1000).
    #[getter(copy)]
    cpu_peak_tenths: u64,
    #[getter(copy)]
    alloc_bytes: u64,
    #[getter(copy)]
    dealloc_bytes: u64,
}

/// One named lock: how often it was taken and how long it was waited for.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters, derive_new::new,
)]
pub struct LockUsage {
    label: String,
    #[getter(copy)]
    count: u64,
    #[getter(copy)]
    wait_avg_ns: u64,
    #[getter(copy)]
    wait_p95_ns: u64,
}

/// One named channel: how many ends were made and how many messages crossed it.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters, derive_new::new,
)]
pub struct ChannelUsage {
    label: String,
    #[getter(copy)]
    instances: u64,
    #[getter(copy)]
    sent: u64,
    #[getter(copy)]
    received: u64,
}

/// Everything in one profile, or the sum of several, besides function timing.
#[derive(Debug, Clone, Default, PartialEq, Eq, derive_getters::Getters)]
pub struct ProfileUsage {
    /// Exact Hotpath data retained separately; heap snapshots/peaks are never summed.
    exact: Vec<hotpath::json::AllocationReport>,
    allocations: Vec<FunctionAllocation>,
    threads: Vec<ThreadUsage>,
    locks: Vec<LockUsage>,
    channels: Vec<ChannelUsage>,
    /// Values the program reported, by key, as the profile last held them.
    values: BTreeMap<String, u64>,
}

/// A byte count as `hotpath` prints it (`221 B`, `163.0 MB`, `2.1 GB`; units are
/// powers of 1024).
#[instrument(level = "trace")]
pub fn parse_bytes(text: &str) -> Option<u64> {
    let (number, unit) = text.trim().split_once(' ')?;
    let value: f64 = number.parse().ok()?;
    let scale = match unit {
        "B" => 1.0,
        "KB" => 1024.0,
        "MB" => 1024.0 * 1024.0,
        "GB" => 1024.0 * 1024.0 * 1024.0,
        "TB" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    Some((value * scale).round() as u64)
}

/// Whether a value counts things exactly and is one cordial owns, so the
/// comparison gates on it.
#[instrument(level = "trace")]
pub fn counts_exactly(key: &str) -> bool {
    EXACT_SUFFIXES.iter().any(|suffix| key.ends_with(suffix))
        && GATED_PREFIXES.iter().any(|prefix| key.starts_with(prefix))
}

/// A percentage such as `103.7%` in tenths of a percent.
#[instrument(level = "trace")]
fn tenths(text: &str) -> Option<u64> {
    let value: f64 = text.trim().trim_end_matches('%').parse().ok()?;
    Some((value * 10.0).round() as u64)
}

/// The rows of a report section, empty when the report has none.
#[instrument(level = "trace", skip(report))]
fn rows<'a>(report: &'a serde_json::Value, section: &str, key: &str) -> &'a [serde_json::Value] {
    report[section][key].as_array().map_or(&[], Vec::as_slice)
}

impl ProfileUsage {
    /// Read the non-timing sections of one `hotpath` JSON report. A section the
    /// program did not feed is absent from the report and reads as empty.
    #[instrument(level = "debug", skip(report))]
    #[hotpath::measure]
    pub fn from_report(report: &serde_json::Value) -> Self {
        let mut allocations = rows(report, "functions_alloc", "data")
            .iter()
            .filter_map(|row| {
                Some(FunctionAllocation::new(
                    row["name"].as_str()?.to_owned(),
                    row["calls"].as_u64()?,
                    parse_bytes(row["total"].as_str()?)?,
                ))
            })
            .collect();
        let mut threads = rows(report, "threads", "data")
            .iter()
            .filter_map(|row| {
                Some(ThreadUsage::new(
                    row["name"].as_str()?.to_owned(),
                    row["cpu_percent_max"]
                        .as_str()
                        .and_then(tenths)
                        .unwrap_or(0),
                    row["alloc_bytes"]
                        .as_str()
                        .and_then(parse_bytes)
                        .unwrap_or(0),
                    row["dealloc_bytes"]
                        .as_str()
                        .and_then(parse_bytes)
                        .unwrap_or(0),
                ))
            })
            .collect();
        let locks = ["mutexes", "rw_locks"]
            .iter()
            .flat_map(|section| rows(report, section, "data"))
            .filter_map(|row| {
                Some(LockUsage::new(
                    row["label"]
                        .as_str()
                        .or_else(|| row["source"].as_str())?
                        .to_owned(),
                    row["count"].as_u64()?,
                    parse_duration(row["wait_avg"].as_str()?)?,
                    parse_duration(row["wait_percentiles"]["p95"].as_str()?)?,
                ))
            })
            .collect();
        let channels = rows(report, "channels", "data")
            .iter()
            .filter_map(|row| {
                Some(ChannelUsage::new(
                    row["label"]
                        .as_str()
                        .or_else(|| row["source"].as_str())?
                        .to_owned(),
                    row["instances"].as_u64()?,
                    row["sent_count"].as_u64()?,
                    row["received_count"].as_u64()?,
                ))
            })
            .collect();
        let values = rows(report, "debug", "entries")
            .iter()
            .filter(|row| row["entry_type"] == "val")
            .filter_map(|row| {
                let value: f64 = row["last_value"].as_str()?.parse().ok()?;
                Some((row["expression"].as_str()?.to_owned(), value.round() as u64))
            })
            .collect();
        let exact: Vec<hotpath::json::AllocationReport> = report
            .get("allocation")
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok())
            .into_iter()
            .collect();
        // The presence of an exact section forbids silently falling back to rounded strings,
        // including a malformed, disabled or unsupported exact producer.
        if report.get("allocation").is_some() {
            allocations = exact
                .iter()
                .filter(|data| data.schema_version == 1)
                .flat_map(|data| data.functions.iter().flatten())
                .filter_map(|row| {
                    Some(FunctionAllocation::new(
                        row.name.clone(),
                        row.calls,
                        row.bytes?,
                    ))
                })
                .collect();
            threads = exact
                .iter()
                .filter(|data| data.schema_version == 1)
                .flat_map(|data| data.threads.iter().flatten())
                .map(|row| {
                    ThreadUsage::new(
                        row.name
                            .clone()
                            .unwrap_or_else(|| format!("tid={}", row.tid)),
                        0,
                        row.allocated_bytes,
                        row.freed_bytes,
                    )
                })
                .collect();
        } else if report["functions_alloc"]["profiling_mode"].as_str() != Some("alloc-bytes") {
            allocations = Vec::new();
        }
        Self {
            exact,
            allocations,
            threads,
            locks,
            channels,
            values,
        }
    }

    /// Several profiles as one: allocations, lock counts and channel traffic add;
    /// a thread keeps its highest processor use; a value keeps its largest, since
    /// a profile reports what a thing came to and the biggest is what a run needed.
    #[instrument(level = "debug", skip(profiles))]
    #[hotpath::measure]
    pub fn merge<'a>(profiles: impl IntoIterator<Item = &'a Self>) -> Self {
        let mut exact = Vec::new();
        let mut allocations: BTreeMap<&str, (u64, u64)> = BTreeMap::new();
        let mut threads: BTreeMap<&str, (u64, u64, u64)> = BTreeMap::new();
        let mut locks: BTreeMap<&str, (u64, u64, u64)> = BTreeMap::new();
        let mut channels: BTreeMap<&str, (u64, u64, u64)> = BTreeMap::new();
        let mut values: BTreeMap<String, u64> = BTreeMap::new();
        for profile in profiles {
            exact.extend(profile.exact.iter().cloned());
            for item in &profile.allocations {
                let entry = allocations.entry(item.name()).or_default();
                entry.0 += item.calls();
                entry.1 += item.total_bytes();
            }
            for item in &profile.threads {
                let entry = threads.entry(item.name()).or_default();
                entry.0 = entry.0.max(item.cpu_peak_tenths());
                entry.1 += item.alloc_bytes();
                entry.2 += item.dealloc_bytes();
            }
            for item in &profile.locks {
                let entry = locks.entry(item.label()).or_default();
                // The average over every taking, from each profile's average.
                entry.1 += item.wait_avg_ns() * item.count();
                entry.0 += item.count();
                entry.2 = entry.2.max(item.wait_p95_ns());
            }
            for item in &profile.channels {
                let entry = channels.entry(item.label()).or_default();
                entry.0 += item.instances();
                entry.1 += item.sent();
                entry.2 += item.received();
            }
            for (key, value) in &profile.values {
                let entry = values.entry(key.clone()).or_default();
                *entry = (*entry).max(*value);
            }
        }
        let mut allocations: Vec<_> = allocations
            .into_iter()
            .map(|(name, (calls, bytes))| FunctionAllocation::new(name.to_owned(), calls, bytes))
            .collect();
        // Exact data stays per producer in RunRecord::memory. Repeats, accounting
        // modes and local thread identities cannot be merged by display name.
        if !exact.is_empty() {
            allocations.clear();
            threads.clear();
        }
        allocations.sort_by_key(|item| std::cmp::Reverse(item.total_bytes()));
        Self {
            exact,
            allocations,
            threads: threads
                .into_iter()
                .map(|(name, (cpu, alloc, dealloc))| {
                    ThreadUsage::new(name.to_owned(), cpu, alloc, dealloc)
                })
                .collect(),
            locks: locks
                .into_iter()
                .map(|(label, (count, waited, p95))| {
                    LockUsage::new(
                        label.to_owned(),
                        count,
                        waited.checked_div(count).unwrap_or(0),
                        p95,
                    )
                })
                .collect(),
            channels: channels
                .into_iter()
                .map(|(label, (instances, sent, received))| {
                    ChannelUsage::new(label.to_owned(), instances, sent, received)
                })
                .collect(),
            values,
        }
    }

    /// The exact counts among the values, named for the record's counters, which
    /// the comparison gates on: a run that stores more files or holds more memos
    /// than the last is a regression whatever the machine.
    #[instrument(level = "trace", skip(self))]
    pub fn exact_counters(&self) -> BTreeMap<String, u64> {
        self.values
            .iter()
            .filter(|(key, _)| counts_exactly(key))
            .map(|(key, value)| (format!("value.{key}"), *value))
            .collect()
    }
}

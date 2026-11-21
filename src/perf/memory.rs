//! Import upstream Hotpath reports without reconstructing heap ownership.
use crate::profiling::{ReportStatus, RunReport};
use serde::{Deserialize, Serialize};
use std::fmt::Write;
use std::path::Path;

/// Availability of allocation traffic, independent of retained-heap attribution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryStatus {
    /// Upstream allocation traffic is available, with its reported precision.
    Available,
    /// Allocation recording was disabled.
    Disabled,
    /// The producer cannot supply supported allocation traffic.
    Unsupported,
    /// Expected capture did not appear.
    Missing,
    /// Invalid report data.
    Malformed,
    /// Report ended before completion.
    Truncated,
    /// Capture did not finish or publish.
    Incomplete,
}

/// One process's report sections and provenance; no allocation counter lives here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryCapture {
    /// Lifecycle, identity, configuration and source paths.
    pub capture: RunReport,
    /// Availability of allocation traffic.
    pub status: MemoryStatus,
    /// Historical fork payload, preserved without claiming upstream support.
    #[serde(default)]
    pub allocation: Option<serde_json::Value>,
    /// Upstream function allocation section, including original formatted values.
    #[serde(default)]
    pub functions: Option<serde_json::Value>,
    /// Upstream thread and RSS section, including original formatted values.
    #[serde(default)]
    pub threads: Option<serde_json::Value>,
    /// Collection errors and incompatible producer metadata.
    pub issues: Vec<String>,
}

impl MemoryCapture {
    /// Read a producer without treating missing or unsupported quantities as zero.
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
            functions: None,
            threads: None,
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
        result.allocation = report
            .get("allocation")
            .filter(|value| !value.is_null())
            .cloned();
        result.functions = report
            .get("functions_alloc")
            .filter(|value| !value.is_null())
            .cloned();
        result.threads = report
            .get("threads")
            .filter(|value| !value.is_null())
            .cloned();
        if !manifest.allocation_enabled {
            result.status = MemoryStatus::Disabled;
        } else if result.allocation.is_some() {
            result.status = MemoryStatus::Unsupported;
            result
                .issues
                .push("historical fork allocation schema; raw evidence preserved".into());
        } else if let Some(functions) = &result.functions {
            let expected = match manifest.traffic_metric.as_str() {
                "bytes" => "alloc-bytes",
                "count" => "alloc-count",
                _ => "unsupported",
            };
            if functions["profiling_mode"].as_str() != Some(expected)
                || functions["data"].as_array().is_none_or(|rows| {
                    rows.iter().any(|row| {
                        row["name"].as_str().is_none()
                            || row["calls"].as_u64().is_none()
                            || row["total"].as_str().is_none()
                    })
                })
            {
                result.status = MemoryStatus::Malformed;
                result
                    .issues
                    .push("allocation section disagrees with producer metric".into());
            }
            if let (Some(total), Some(included)) = (
                functions["total_count"].as_u64(),
                functions["included_count"].as_u64(),
            ) && included < total
            {
                result.issues.push(format!(
                    "function report truncated: {included}/{total} rows"
                ));
            }
        } else {
            result.status = MemoryStatus::Unsupported;
            result
                .issues
                .push("upstream allocation section unavailable".into());
        }
        result
    }

    /// Upstream traffic reports do not prove full retained-heap attribution.
    pub fn attribution_complete(&self) -> bool {
        false
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

/// Render reported traffic and process RSS separately, preserving producer precision.
pub fn render_memory(captures: &[MemoryCapture], top: usize) -> String {
    let mut out = format!(
        "\nMemory: {} process captures; allocation traffic, not retained heap.\n",
        captures.len()
    );
    for item in captures {
        let _ = writeln!(
            out,
            "  {} {}: {:?}",
            item.capture.process_id,
            item.key(),
            item.status
        );
        if let Some(manifest) = &item.capture.manifest {
            let _ = writeln!(
                out,
                "    Hotpath {}; {} {}; boundary {}",
                manifest.hotpath_version,
                manifest.accounting,
                manifest.traffic_metric,
                manifest.identity.boundary
            );
        }
        if let Some(functions) = &item.functions
            && let Some(rows) = functions["data"].as_array()
        {
            for row in rows.iter().take(if top == 0 { usize::MAX } else { top }) {
                let _ = writeln!(
                    out,
                    "    {}: {} in {} calls",
                    row["name"].as_str().unwrap_or("?"),
                    row["total"].as_str().unwrap_or("unavailable"),
                    row["calls"]
                );
            }
        }
        if let Some(threads) = &item.threads {
            let _ = writeln!(
                out,
                "    RSS: {}; OS high-water: {}",
                threads["rss_bytes"].as_str().unwrap_or("unavailable"),
                threads["rss_bytes_max"].as_str().unwrap_or("unavailable")
            );
        }
        for issue in &item.issues {
            let _ = writeln!(out, "    {issue}");
        }
    }
    let _ = writeln!(
        out,
        "  Reported byte strings retain their precision; rounded units are approximate. Retained ownership and simultaneous heap peak are unavailable. Native/mmap/stack memory and opaque dependency work remain coverage limits."
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

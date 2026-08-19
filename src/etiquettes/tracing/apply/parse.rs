//! Parse open items from `tracing-instrument.checklist.md`.

use std::path::{Path, PathBuf};

use tracing::instrument;

use crate::error::CordialResult;

use super::InstrumentGap;

/// Parse open items from `tracing-instrument.checklist.md`.
#[instrument(level = "debug", skip(path), err(level = "warn"))]
pub fn parse_tracing_instrument_checklist(path: &Path) -> CordialResult<Vec<InstrumentGap>> {
    let body = std::fs::read_to_string(path)?;
    Ok(parse_tracing_instrument_checklist_text(&body))
}

/// Parse checklist markdown already loaded into memory.
#[instrument(level = "debug")]
pub fn parse_tracing_instrument_checklist_text(body: &str) -> Vec<InstrumentGap> {
    let mut gaps = Vec::new();
    let mut current_crate = String::new();

    for line in body.lines() {
        if let Some(crate_name) = parse_crate_heading(line) {
            current_crate = crate_name;
            continue;
        }
        if current_crate.is_empty() {
            continue;
        }
        if let Some((qualified_name, rel_path, line_number)) = parse_gap_line(line) {
            gaps.push(InstrumentGap {
                crate_name: current_crate.clone(),
                qualified_name,
                rel_path: PathBuf::from(rel_path),
                line: line_number,
            });
        }
    }

    gaps
}

#[instrument(level = "debug")]
fn parse_crate_heading(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let rest = trimmed.strip_prefix("## `")?;
    let crate_name = rest.strip_suffix('`')?;
    Some(crate_name.to_string())
}

#[instrument(level = "debug")]
fn parse_gap_line(line: &str) -> Option<(String, String, u32)> {
    let trimmed = line.trim();
    if !trimmed.starts_with("- [ ] `") {
        return None;
    }
    let parts: Vec<&str> = trimmed.split('`').collect();
    if parts.len() < 4 {
        return None;
    }
    let qualified_name = parts[1].to_string();
    let location = parts[3];
    let (rel_path, line_text) = location.rsplit_once(':')?;
    let line_number = line_text.parse().ok()?;
    Some((qualified_name, rel_path.to_string(), line_number))
}

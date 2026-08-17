//! Apply `#[instrument]` attributes from a tracing instrument checklist.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use tracing::instrument;

use crate::error::{CordialError, CordialResult};
use crate::loader::CrateTarget;
use crate::session::RunAll;
use crate::targets::discover_crate_targets;

/// One open checklist row targeting a function or method.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstrumentGap {
    pub crate_name: String,
    pub qualified_name: String,
    pub rel_path: PathBuf,
    pub line: u32,
}

/// Result of applying instrumentation patches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstrumentApplySummary {
    pub changed_functions: usize,
    pub changed_files: usize,
    pub skipped_existing: usize,
    pub unresolved: usize,
}

/// Parameter names that should appear in `#[instrument(skip(...))]`.
const SKIP_PARAM_NAMES: &[&str] = &[
    "binary_entry_files",
    "build",
    "cache",
    "conn",
    "connection",
    "ctx",
    "data",
    "err",
    "error",
    "file",
    "findings",
    "formatter",
    "inventory",
    "items",
    "key",
    "kind",
    "manifest",
    "msg",
    "options",
    "path",
    "report",
    "self",
    "snapshots",
    "source",
    "syntax",
    "ui",
    "workspace",
];

/// Parse open items from `tracing-instrument.checklist.md`.
#[instrument(skip(path))]
pub fn parse_tracing_instrument_checklist(path: &Path) -> CordialResult<Vec<InstrumentGap>> {
    let body = std::fs::read_to_string(path)?;
    Ok(parse_tracing_instrument_checklist_text(&body))
}

/// Parse checklist markdown already loaded into memory.
#[instrument(skip(body))]
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

/// Patch source files listed in the tracing instrument checklist.
#[instrument(skip(project_root, checklist_path, only_crate))]
pub fn run_tracing_instrument_apply(
    project_root: &Path,
    checklist_path: &Path,
    only_crate: Option<&str>,
    dry_run: bool,
) -> CordialResult<InstrumentApplySummary> {
    let gaps = parse_tracing_instrument_checklist(checklist_path)?;
    if gaps.is_empty() {
        return Err(CordialError::invariant(
            "no open checklist items found in tracing instrument checklist",
        ));
    }

    let filter = RunAll;
    let targets = discover_crate_targets(project_root, &filter)?;
    let crate_roots: HashMap<String, PathBuf> = targets
        .into_iter()
        .map(|target: CrateTarget| (target.crate_name, target.crate_root))
        .collect();

    let mut by_file: BTreeMap<(String, PathBuf), Vec<InstrumentGap>> = BTreeMap::new();
    for gap in gaps {
        if only_crate.is_some_and(|name| name != gap.crate_name) {
            continue;
        }
        if !crate_roots.contains_key(&gap.crate_name) {
            tracing::warn!(
                crate_name = %gap.crate_name,
                "skipping gap for crate not in scan targets"
            );
            continue;
        }
        by_file
            .entry((gap.crate_name.clone(), gap.rel_path.clone()))
            .or_default()
            .push(gap);
    }

    let mut summary = InstrumentApplySummary {
        changed_functions: 0,
        changed_files: 0,
        skipped_existing: 0,
        unresolved: 0,
    };

    for ((crate_name, rel_path), mut file_gaps) in by_file {
        let Some(crate_root) = crate_roots.get(&crate_name) else {
            continue;
        };
        let path = crate_root.join(&rel_path);
        if !path.is_file() {
            tracing::warn!(
                path = %path.display(),
                crate_name = %crate_name,
                "checklist path does not exist"
            );
            summary.unresolved += file_gaps.len();
            continue;
        }

        let mut lines: Vec<String> = std::fs::read_to_string(&path)?
            .lines()
            .map(str::to_string)
            .collect();
        file_gaps.sort_by_key(|right| std::cmp::Reverse(right.line));

        let mut file_changed = false;
        for gap in file_gaps {
            match apply_gap(&mut lines, &gap) {
                GapApplyOutcome::Applied => {
                    summary.changed_functions += 1;
                    file_changed = true;
                }
                GapApplyOutcome::AlreadyInstrumented => {
                    summary.skipped_existing += 1;
                }
                GapApplyOutcome::Unresolved => {
                    summary.unresolved += 1;
                }
            }
        }

        if file_changed {
            lines = ensure_use_instrument(lines);
            if dry_run {
                tracing::info!(path = %path.display(), "dry run: would update file");
            } else {
                std::fs::write(&path, format!("{}\n", lines.join("\n")))?;
            }
            summary.changed_files += 1;
        }
    }

    tracing::info!(
        changed_functions = summary.changed_functions,
        changed_files = summary.changed_files,
        skipped_existing = summary.skipped_existing,
        unresolved = summary.unresolved,
        dry_run,
        "tracing instrument apply complete"
    );
    Ok(summary)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GapApplyOutcome {
    Applied,
    AlreadyInstrumented,
    Unresolved,
}

#[instrument(skip(line))]
fn parse_crate_heading(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let rest = trimmed.strip_prefix("## `")?;
    let crate_name = rest.strip_suffix('`')?;
    Some(crate_name.to_string())
}

#[instrument(skip(line))]
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

#[instrument(skip(lines, gap))]
fn apply_gap(lines: &mut Vec<String>, gap: &InstrumentGap) -> GapApplyOutcome {
    let Some(fn_idx) = find_fn_line(lines, gap.line, &gap.qualified_name) else {
        tracing::warn!(
            path = %gap.rel_path.display(),
            line = gap.line,
            qualified_name = %gap.qualified_name,
            "no fn near checklist line"
        );
        return GapApplyOutcome::Unresolved;
    };
    if has_instrument(lines, fn_idx) {
        return GapApplyOutcome::AlreadyInstrumented;
    }

    let attr_indices = collect_attr_indices(lines, fn_idx);
    let indent = leading_indent(&lines[fn_idx]);
    let indent = attr_indices
        .first()
        .map(|idx| leading_indent(&lines[*idx]))
        .unwrap_or(indent);

    let param_names = parse_param_names(&signature_text(lines, fn_idx));
    let instrument_line = build_instrument_line(&param_names, &indent);

    let insert_at = insert_after_track_caller(lines, &attr_indices).unwrap_or(fn_idx);
    lines.insert(insert_at, instrument_line);
    GapApplyOutcome::Applied
}

#[instrument(skip(lines, qualified_name))]
fn find_fn_line(lines: &[String], target_line: u32, qualified_name: &str) -> Option<usize> {
    let idx = target_line.saturating_sub(1) as usize;
    let start = idx.saturating_sub(12);
    let end = (idx + 12).min(lines.len());
    let expected_name = local_fn_name(qualified_name);

    let mut candidates: Vec<(usize, bool, usize)> = Vec::new();
    for (i, line) in lines.iter().enumerate().skip(start).take(end - start) {
        if line.trim().starts_with("//") {
            continue;
        }
        let Some(name) = extract_fn_name(line) else {
            continue;
        };
        let name_mismatch = name != expected_name;
        candidates.push((i.abs_diff(idx), name_mismatch, i));
    }

    candidates.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));
    candidates.first().map(|candidate| candidate.2)
}

#[instrument(skip(qualified_name))]
fn local_fn_name(qualified_name: &str) -> &str {
    qualified_name.rsplit("::").next().unwrap_or(qualified_name)
}

#[instrument(skip(line))]
fn extract_fn_name(line: &str) -> Option<&str> {
    let fn_idx = line.find("fn ")?;
    let rest = &line[fn_idx + 3..];
    let end = rest.find(['(', '<']).unwrap_or(rest.len());
    let name = rest[..end].trim();
    if name.is_empty()
        || !name
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
    {
        return None;
    }
    Some(name)
}

#[instrument(skip(lines))]
fn collect_attr_indices(lines: &[String], fn_idx: usize) -> Vec<usize> {
    let mut indices = Vec::new();
    let mut i = fn_idx;
    while i > 0 {
        i -= 1;
        let stripped = lines[i].trim();
        if stripped.starts_with("#[") {
            indices.insert(0, i);
            continue;
        }
        if stripped.is_empty() || stripped.starts_with("///") || stripped.starts_with("//") {
            continue;
        }
        break;
    }
    indices
}

#[instrument(skip(lines))]
fn has_instrument(lines: &[String], fn_idx: usize) -> bool {
    collect_attr_indices(lines, fn_idx)
        .iter()
        .any(|idx| lines[*idx].contains("instrument"))
}

#[instrument(skip(lines))]
fn signature_text(lines: &[String], fn_idx: usize) -> String {
    let mut chunks = Vec::new();
    for line in lines
        .iter()
        .take(fn_idx.saturating_add(24).min(lines.len()))
        .skip(fn_idx)
    {
        chunks.push(line.as_str());
        let joined = chunks.join(" ");
        if line.contains('{')
            || (joined.matches('(').count() <= joined.matches(')').count()
                && joined.contains('(')
                && joined.contains(')'))
        {
            return joined;
        }
    }
    chunks.join(" ")
}

#[instrument(skip(signature))]
fn parse_param_names(signature: &str) -> Vec<String> {
    let Some(blob) = extract_fn_param_blob(signature) else {
        return Vec::new();
    };
    let mut names = Vec::new();
    for raw_param in split_params(blob) {
        let param = raw_param.split_whitespace().collect::<Vec<_>>().join(" ");
        if param.is_empty() {
            continue;
        }
        if param == "&self" || param == "&mut self" || param == "self" || param.ends_with(" self") {
            names.push("self".to_string());
            continue;
        }
        if let Some(name) = param_name_from_typed_param(&param) {
            names.push(name);
        }
    }
    names
}

#[instrument(skip(param))]
fn param_name_from_typed_param(param: &str) -> Option<String> {
    let trimmed = param.trim();
    let trimmed = trimmed.strip_prefix("mut ").unwrap_or(trimmed);
    let (name, _) = trimmed.split_once(':')?;
    Some(name.trim().to_string())
}

#[instrument(skip(signature))]
fn extract_fn_param_blob(signature: &str) -> Option<&str> {
    let fn_idx = signature.find("fn ")?;
    let after_fn = &signature[fn_idx + 3..];
    let paren_start = after_fn.find('(')?;
    let params_start = fn_idx + 3 + paren_start + 1;
    let mut depth = 1usize;
    let mut i = params_start;
    let bytes = signature.as_bytes();
    while i < bytes.len() && depth > 0 {
        match bytes[i] as char {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        i += 1;
    }
    if depth != 0 {
        return None;
    }
    Some(&signature[params_start..i - 1])
}

#[instrument(skip(blob))]
fn split_params(blob: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut depth_angle = 0usize;
    let mut depth_paren = 0usize;

    for ch in blob.chars() {
        match ch {
            '<' => depth_angle += 1,
            '>' => depth_angle = depth_angle.saturating_sub(1),
            '(' => depth_paren += 1,
            ')' => depth_paren = depth_paren.saturating_sub(1),
            ',' if depth_angle == 0 && depth_paren == 0 => {
                parts.push(std::mem::take(&mut current));
                continue;
            }
            _ => {}
        }
        current.push(ch);
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}

#[instrument(skip(param_names, indent))]
fn build_instrument_line(param_names: &[String], indent: &str) -> String {
    let skip_names: Vec<&str> = param_names
        .iter()
        .map(String::as_str)
        .filter(|name| SKIP_PARAM_NAMES.contains(name))
        .collect();
    if skip_names.is_empty() {
        format!("{indent}#[instrument]")
    } else {
        format!("{indent}#[instrument(skip({}))]", skip_names.join(", "))
    }
}

#[instrument(skip(lines, attr_indices))]
fn insert_after_track_caller(lines: &[String], attr_indices: &[usize]) -> Option<usize> {
    for idx in attr_indices.iter().rev() {
        if lines[*idx].contains("track_caller") {
            return Some(idx + 1);
        }
    }
    attr_indices.first().copied()
}

#[instrument(skip(line))]
fn leading_indent(line: &str) -> String {
    line.chars()
        .take_while(|ch| *ch == ' ' || *ch == '\t')
        .collect()
}

#[instrument(skip(lines))]
fn ensure_use_instrument(lines: Vec<String>) -> Vec<String> {
    let joined = lines.join("\n");
    if joined.contains("use tracing::instrument;") || joined.contains("#[tracing::instrument") {
        return lines;
    }

    let mut insert_at = 0usize;
    let mut i = 0usize;
    while i < lines.len() {
        let stripped = lines[i].trim();
        if stripped.starts_with("#!") || stripped.starts_with("//!") || stripped.is_empty() {
            insert_at = i + 1;
            i += 1;
            continue;
        }
        if stripped.starts_with("#[") {
            insert_at = i + 1;
            i += 1;
            continue;
        }
        if stripped.starts_with("use ") {
            while i < lines.len() && !lines[i].contains(';') {
                i += 1;
            }
            insert_at = i + 1;
            i += 1;
            continue;
        }
        break;
    }

    let mut out = lines;
    out.insert(insert_at, "use tracing::instrument;".to_string());
    out
}

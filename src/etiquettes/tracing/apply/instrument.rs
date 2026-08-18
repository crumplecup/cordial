//! Insert or rewrite `#[instrument]` from a classified recipe.

use tracing::instrument;

use super::super::types::{FunctionRecord, InstrumentRecipe};
use super::InstrumentGap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GapApplyOutcome {
    Applied,
    AlreadyInstrumented,
    Unresolved,
}

#[instrument(skip(lines, gap, recipe))]
pub(super) fn apply_gap(
    lines: &mut Vec<String>,
    gap: &InstrumentGap,
    recipe: &InstrumentRecipe,
) -> GapApplyOutcome {
    let Some(fn_idx) = find_fn_line(lines, gap.line, &gap.qualified_name) else {
        tracing::warn!(
            path = %gap.rel_path.display(),
            line = gap.line,
            qualified_name = %gap.qualified_name,
            "no fn near checklist line"
        );
        return GapApplyOutcome::Unresolved;
    };

    let attr = recipe.as_attribute();
    if let Some((start, end)) = instrument_attr_range(lines, fn_idx) {
        if attrs_match_recipe(&lines[start..=end], &attr) {
            return GapApplyOutcome::AlreadyInstrumented;
        }
        let indent = leading_indent(&lines[start]);
        lines.drain(start..=end);
        lines.insert(start, format!("{indent}{attr}"));
        return GapApplyOutcome::Applied;
    }

    let attr_indices = collect_attr_indices(lines, fn_idx);
    let indent = leading_indent(&lines[fn_idx]);
    let indent = attr_indices
        .first()
        .map(|idx| leading_indent(&lines[*idx]))
        .unwrap_or(indent);
    let insert_at = insert_after_track_caller(lines, &attr_indices).unwrap_or(fn_idx);
    lines.insert(insert_at, format!("{indent}{attr}"));
    GapApplyOutcome::Applied
}

#[instrument(skip(records, gap))]
pub(super) fn recipe_for_gap<'a>(
    records: &'a [FunctionRecord],
    gap: &InstrumentGap,
) -> Option<&'a InstrumentRecipe> {
    if let Some(record) = records
        .iter()
        .find(|record| record.qualified_name == gap.qualified_name)
    {
        return Some(&record.recipe);
    }

    let local = local_fn_name(&gap.qualified_name);
    let rel = gap.rel_path.to_string_lossy().replace('\\', "/");
    records
        .iter()
        .filter(|record| {
            local_fn_name(&record.qualified_name) == local && file_matches(&record.file, &rel)
        })
        .min_by_key(|record| record.line.abs_diff(gap.line))
        .filter(|record| record.line.abs_diff(gap.line) <= 24)
        .map(|record| &record.recipe)
}

fn file_matches(record_file: &str, gap_file: &str) -> bool {
    record_file == gap_file || record_file.ends_with(gap_file) || gap_file.ends_with(record_file)
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
fn instrument_attr_range(lines: &[String], fn_idx: usize) -> Option<(usize, usize)> {
    let attrs = collect_attr_indices(lines, fn_idx);
    for &start in &attrs {
        if !lines[start].contains("instrument") {
            continue;
        }
        let mut end = start;
        while end < fn_idx && !lines[end].contains(']') {
            end += 1;
        }
        if end >= fn_idx {
            return None;
        }
        return Some((start, end));
    }
    None
}

fn attrs_match_recipe(attr_lines: &[String], recipe_attr: &str) -> bool {
    normalize_attr(&attr_lines.join(" ")) == normalize_attr(recipe_attr)
}

fn normalize_attr(text: &str) -> String {
    text.split_whitespace()
        .collect::<String>()
        .replace("#[tracing::instrument", "#[instrument")
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
pub(super) fn ensure_use_instrument(lines: Vec<String>) -> Vec<String> {
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

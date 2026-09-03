//! Insert or rewrite `#[instrument]` from a classified recipe.

use tracing::instrument;

use crate::error::{CordialError, CordialResult};

use super::super::types::{FunctionRecord, InstrumentRecipe};
use super::InstrumentGap;
use super::verifier_policy::{TracingApplyPolicy, gate_predicate};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GapApplyOutcome {
    Applied,
    AlreadyInstrumented,
    Unresolved,
    /// Left untouched on purpose: the real verifier toolchain for every
    /// crate that compiles this file can't tolerate `#[instrument]` at
    /// all, gated or not (see [`TracingApplyPolicy::Skip`]). The
    /// checklist item stays open when there was nothing to strip.
    SkippedPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum InstrumentAttrStyle {
    Short,
    Path,
    CratePath,
}

#[instrument(level = "debug")]
pub(super) fn attr_style(lines: &[String]) -> InstrumentAttrStyle {
    if file_declares_mod(lines, "tracing") {
        InstrumentAttrStyle::CratePath
    } else if file_declares_mod(lines, "instrument") {
        InstrumentAttrStyle::Path
    } else {
        InstrumentAttrStyle::Short
    }
}

#[instrument(level = "debug", skip(gap, recipe, style, policy), err(level = "warn"))]
pub(super) fn apply_gap(
    lines: &mut Vec<String>,
    gap: &InstrumentGap,
    recipe: &InstrumentRecipe,
    style: InstrumentAttrStyle,
    policy: &TracingApplyPolicy,
) -> CordialResult<GapApplyOutcome> {
    let Some(fn_idx) = find_fn_line(lines, gap.line(), gap.qualified_name()) else {
        tracing::warn!(
            path = %gap.rel_path().display(),
            line = gap.line(),
            qualified_name = %gap.qualified_name(),
            "no fn near checklist line"
        );
        return Ok(GapApplyOutcome::Unresolved);
    };

    // Skip is not a write policy: the caller strips via `strip_instrument`.
    // Returning an error keeps control in that decision chain instead of aborting.
    let attr = match policy {
        TracingApplyPolicy::Bare => recipe_attr(recipe, style),
        TracingApplyPolicy::Gated(cfgs) => {
            // Always fully qualified (`tracing::instrument`, or
            // `::tracing::instrument` when `tracing` itself is
            // shadowed), never the short form -- a gated function is
            // often nested inside an outer `#[cfg(<verifier>)]` item
            // (a real Kani proof harness/contract wrapper, say), so
            // it's never reachable under an ordinary build at all;
            // relying on a plain `use tracing::instrument;` for it
            // would make that import `unused_imports`-flagged in any
            // file where nothing else needs the short form. The
            // qualified form never depends on that import existing.
            let qualified = if style == InstrumentAttrStyle::CratePath {
                recipe.as_crate_path_attribute()
            } else {
                recipe.as_path_attribute()
            };
            gate_attr(&qualified, &gate_predicate(cfgs))
        }
        TracingApplyPolicy::Skip => {
            return Err(CordialError::unreachable(
                "apply_gap is the write path; Skip is handled by strip_instrument",
            ));
        }
    };
    if let Some((start, end)) = instrument_attr_range(lines, fn_idx) {
        if attrs_match_recipe(&lines[start..=end], &attr) {
            return Ok(GapApplyOutcome::AlreadyInstrumented);
        }
        let indent = leading_indent(&lines[start]);
        lines.drain(start..=end);
        lines.insert(start, format!("{indent}{attr}"));
        return Ok(GapApplyOutcome::Applied);
    }

    let attr_indices = collect_attr_indices(lines, fn_idx);
    let indent = leading_indent(&lines[fn_idx]);
    let indent = attr_indices
        .first()
        .map(|idx| leading_indent(&lines[*idx]))
        .unwrap_or(indent);
    let insert_at = insert_after_track_caller(lines, &attr_indices).unwrap_or(fn_idx);
    lines.insert(insert_at, format!("{indent}{attr}"));
    Ok(GapApplyOutcome::Applied)
}

/// Remove an existing `#[instrument]` / `#[cfg_attr(.., instrument)]` from
/// `gap` — attenuation for proof-only functions and skip-policy files.
#[instrument(level = "debug", skip(lines, gap))]
pub(super) fn strip_instrument(lines: &mut Vec<String>, gap: &InstrumentGap) -> GapApplyOutcome {
    let Some(fn_idx) = find_fn_line(lines, gap.line(), gap.qualified_name()) else {
        tracing::warn!(
            path = %gap.rel_path().display(),
            line = gap.line(),
            qualified_name = %gap.qualified_name(),
            "no fn near checklist line"
        );
        return GapApplyOutcome::Unresolved;
    };
    let Some((start, end)) = instrument_attr_range(lines, fn_idx) else {
        return GapApplyOutcome::SkippedPolicy;
    };
    lines.drain(start..=end);
    GapApplyOutcome::Applied
}

/// Wrap a rendered `#[instrument(..)]` (or path-qualified equivalent) as
/// `#[cfg_attr(not(#predicate), ..)]` -- the real toolchain still never
/// sees `#[instrument]` under the gated cfg (e.g. `cargo kani`'s
/// `--cfg kani`), because `cfg_attr`'s condition is evaluated before its
/// inner attribute is ever expanded.
#[instrument(level = "trace")]
fn gate_attr(attr: &str, predicate: &str) -> String {
    let inner = attr
        .strip_prefix('#')
        .and_then(|rest| rest.strip_prefix('['))
        .and_then(|rest| rest.strip_suffix(']'))
        .unwrap_or(attr);
    format!("#[cfg_attr(not({predicate}), {inner})]")
}

#[instrument(level = "debug", skip(recipe, style))]
fn recipe_attr(recipe: &InstrumentRecipe, style: InstrumentAttrStyle) -> String {
    match style {
        InstrumentAttrStyle::Short => recipe.as_attribute(),
        InstrumentAttrStyle::Path => recipe.as_path_attribute(),
        InstrumentAttrStyle::CratePath => recipe.as_crate_path_attribute(),
    }
}

#[instrument(level = "debug", skip(records, gap))]
pub(super) fn recipe_for_gap<'a>(
    records: &'a [FunctionRecord],
    gap: &InstrumentGap,
) -> Option<&'a InstrumentRecipe> {
    let rel = gap.rel_path().to_string_lossy().replace('\\', "/");
    let named: Vec<_> = records
        .iter()
        .filter(|record| record.qualified_name() == gap.qualified_name())
        .collect();
    if let Some(record) = named
        .iter()
        .copied()
        .filter(|record| file_matches(record.file(), &rel))
        .min_by_key(|record| record.line().abs_diff(gap.line()))
    {
        return Some(record.recipe());
    }
    if let Some(record) = named.first().copied() {
        return Some(record.recipe());
    }

    let local = local_fn_name(gap.qualified_name());
    records
        .iter()
        .filter(|record| {
            local_fn_name(record.qualified_name()) == local && file_matches(record.file(), &rel)
        })
        .min_by_key(|record| record.line().abs_diff(gap.line()))
        .filter(|record| record.line().abs_diff(gap.line()) <= 24)
        .map(|record| record.recipe())
}

#[instrument(level = "debug")]
fn file_matches(record_file: &str, gap_file: &str) -> bool {
    record_file == gap_file || record_file.ends_with(gap_file) || gap_file.ends_with(record_file)
}

#[instrument(level = "debug")]
fn find_fn_line(lines: &[String], target_line: u32, qualified_name: &str) -> Option<usize> {
    let idx = target_line.saturating_sub(1) as usize;
    let expected_name = local_fn_name(qualified_name);

    let mut named: Vec<(usize, usize)> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if line.trim().starts_with("//") {
            continue;
        }
        let Some(name) = extract_fn_name(line) else {
            continue;
        };
        if name != expected_name {
            continue;
        }
        named.push((i.abs_diff(idx), i));
    }
    named.sort_by_key(|candidate| candidate.0);
    named.first().map(|candidate| candidate.1)
}

#[instrument(level = "debug")]
fn local_fn_name(qualified_name: &str) -> &str {
    qualified_name.rsplit("::").next().unwrap_or(qualified_name)
}

#[instrument(level = "debug")]
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

/// Net `)`/`]` minus `(`/`[` on one line -- positive means the line closes
/// more delimiters than it opens (a mid/tail continuation line of a
/// multi-line attribute, e.g. `not(kani),` or the trailing `)]`).
#[instrument(level = "trace", ret)]
fn closes_minus_opens(line: &str) -> i32 {
    let mut delta = 0i32;
    for ch in line.chars() {
        match ch {
            '(' | '[' => delta -= 1,
            ')' | ']' => delta += 1,
            _ => {}
        }
    }
    delta
}

/// Every line spanned by the attribute block directly above `fn_idx`,
/// including every physical line of a multi-line `#[cfg_attr(\n  ...,\n
/// ...\n)]` (bracket-depth aware, not just lines literally starting with
/// `#[` -- a continuation line like `not(kani),` or the attribute's own
/// `tracing::instrument(...)` payload line never does).
#[instrument(level = "debug")]
fn collect_attr_indices(lines: &[String], fn_idx: usize) -> Vec<usize> {
    let mut indices = Vec::new();
    let mut i = fn_idx;
    let mut pending: i32 = 0;
    while i > 0 {
        i -= 1;
        let stripped = lines[i].trim();
        let delta = closes_minus_opens(stripped);
        if pending > 0 {
            // Still inside an unmatched multi-line attribute opened
            // (in file order) below this line.
            indices.insert(0, i);
            pending += delta;
            continue;
        }
        if stripped.starts_with("#[") {
            indices.insert(0, i);
            pending += delta;
            continue;
        }
        if stripped.is_empty() || stripped.starts_with("///") || stripped.starts_with("//") {
            continue;
        }
        if delta > 0 {
            // A closing-only tail line (e.g. bare `)]`) that doesn't
            // itself start with `#[` -- still part of the attribute
            // above it.
            indices.insert(0, i);
            pending = delta;
            continue;
        }
        break;
    }
    indices
}

#[instrument(level = "debug")]
fn instrument_attr_range(lines: &[String], fn_idx: usize) -> Option<(usize, usize)> {
    let attrs = collect_attr_indices(lines, fn_idx);
    for &start in &attrs {
        if !lines[start].trim_start().starts_with("#[") {
            continue;
        }
        // The block of lines this specific attribute (starting at
        // `start`) owns, out of every attribute line collected above --
        // walk forward accumulating bracket depth until it returns to
        // zero, not just "the next line containing `]`" (a nested `[`
        // inside the attribute's own arguments, e.g. `fields(x = ?v[0])`,
        // would otherwise close too early).
        let mut end = start;
        let mut depth = -closes_minus_opens(lines[start].trim());
        while depth > 0 && end + 1 < fn_idx {
            end += 1;
            depth -= closes_minus_opens(lines[end].trim());
        }
        if depth > 0 {
            continue;
        }
        let block = lines[start..=end].join(" ");
        if !block.contains("instrument") {
            continue;
        }
        return Some((start, end));
    }
    None
}

#[instrument(level = "debug")]
fn attrs_match_recipe(attr_lines: &[String], recipe_attr: &str) -> bool {
    normalize_attr(&attr_lines.join(" ")) == normalize_attr(recipe_attr)
}

#[instrument(level = "debug")]
fn normalize_attr(text: &str) -> String {
    text.split_whitespace()
        .collect::<String>()
        .replace("#[::tracing::instrument", "#[instrument")
        .replace("#[tracing::instrument", "#[instrument")
}

#[instrument(level = "debug")]
fn insert_after_track_caller(lines: &[String], attr_indices: &[usize]) -> Option<usize> {
    for idx in attr_indices.iter().rev() {
        if lines[*idx].contains("track_caller") {
            return Some(idx + 1);
        }
    }
    attr_indices.first().copied()
}

#[instrument(level = "debug")]
fn leading_indent(line: &str) -> String {
    line.chars()
        .take_while(|ch| *ch == ' ' || *ch == '\t')
        .collect()
}

#[instrument(level = "debug")]
fn file_declares_mod(lines: &[String], name: &str) -> bool {
    lines.iter().any(|line| is_mod_decl(line.trim(), name))
}

#[instrument(level = "trace", ret)]
fn is_mod_decl(stripped: &str, name: &str) -> bool {
    let rest = stripped
        .strip_prefix("pub(crate) ")
        .or_else(|| stripped.strip_prefix("pub(super) "))
        .or_else(|| stripped.strip_prefix("pub "))
        .unwrap_or(stripped);
    rest == format!("mod {name};") || rest.starts_with(&format!("mod {name} {{"))
}

#[instrument(level = "debug")]
pub(super) fn ensure_use_instrument(lines: Vec<String>) -> Vec<String> {
    if attr_style(&lines) != InstrumentAttrStyle::Short || tracing_use_includes_instrument(&lines) {
        return lines;
    }

    let mut insert_at = 0usize;
    let mut i = 0usize;
    while i < lines.len() {
        let stripped = lines[i].trim();
        if stripped.starts_with("///") || stripped.starts_with("#[") {
            break;
        }
        if stripped.starts_with("#!")
            || stripped.starts_with("//!")
            || stripped.starts_with("//")
            || stripped.is_empty()
        {
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

#[instrument(level = "debug")]
fn tracing_use_includes_instrument(lines: &[String]) -> bool {
    let mut i = 0usize;
    while i < lines.len() {
        let stripped = lines[i].trim();
        if stripped.starts_with("use tracing::") {
            let mut block = stripped.to_string();
            while i < lines.len() && !lines[i].contains(';') {
                i += 1;
                if i < lines.len() {
                    block.push(' ');
                    block.push_str(lines[i].trim());
                }
            }
            if import_names_include_instrument(&block) {
                return true;
            }
        }
        i += 1;
    }
    false
}

#[instrument(level = "debug")]
fn import_names_include_instrument(block: &str) -> bool {
    if block.contains("use tracing::instrument;") || block.contains("use tracing::instrument as ") {
        return true;
    }
    let Some(start) = block.find('{') else {
        return false;
    };
    let Some(end) = block[start + 1..].find('}') else {
        return false;
    };
    block[start + 1..start + 1 + end]
        .split(',')
        .any(|name| name.trim() == "instrument")
}

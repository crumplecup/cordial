//! Insert `#[instrument]` on a function near a checklist line.

use tracing::instrument;

use super::InstrumentGap;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GapApplyOutcome {
    Applied,
    AlreadyInstrumented,
    Unresolved,
}

#[instrument(skip(lines, gap))]
pub(super) fn apply_gap(lines: &mut Vec<String>, gap: &InstrumentGap) -> GapApplyOutcome {
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

//! Invoke Verus and parse rustc-style `warning:` diagnostics.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::CordialResult;

use super::types::{VerusWarningRecord, VerusWarningRuleId};

use tracing::instrument;

/// Scan a crate: run Verus when this crate is a Verus target and the
/// compiler is on `PATH`.
#[instrument(level = "debug", err(level = "warn"))]
pub fn scan_crate_verus_warnings(crate_root: &Path) -> CordialResult<Vec<VerusWarningRecord>> {
    if !crate_is_verus_target(crate_root) {
        return Ok(Vec::new());
    }
    let Some(verus) = resolve_verus_binary() else {
        tracing::debug!(
            crate_root = %crate_root.display(),
            "verus binary not found; skipping Verus warning scan"
        );
        return Ok(Vec::new());
    };
    let Some(entry) = verus_crate_entry(crate_root) else {
        return Ok(Vec::new());
    };
    let output = run_verus(&verus, crate_root, &entry)?;
    Ok(parse_verus_compiler_output(&output, crate_root))
}

/// True when this member is meant to be compiled with the Verus rustc fork.
#[instrument(level = "debug")]
pub fn crate_is_verus_target(crate_root: &Path) -> bool {
    if crate_root
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with("_verus"))
    {
        return true;
    }
    let Ok(manifest) = std::fs::read_to_string(crate_root.join("Cargo.toml")) else {
        return false;
    };
    manifest_names_verus_dep(&manifest)
}

#[instrument(level = "debug", skip(manifest))]
fn manifest_names_verus_dep(manifest: &str) -> bool {
    let mut in_deps = false;
    for raw in manifest.lines() {
        let line = raw.trim();
        if line.starts_with('[') {
            in_deps = matches!(
                line,
                "[dependencies]"
                    | "[dev-dependencies]"
                    | "[build-dependencies]"
                    | "[workspace.dependencies]"
            );
            continue;
        }
        if !in_deps || line.is_empty() || line.starts_with('#') {
            continue;
        }
        let key = line
            .split(['=', '.', ' '])
            .next()
            .unwrap_or("")
            .trim()
            .trim_matches('"');
        if matches!(key, "vstd" | "verus_builtin" | "verus_builtin_macros") {
            return true;
        }
    }
    false
}

#[instrument(level = "debug")]
fn verus_crate_entry(crate_root: &Path) -> Option<VerusEntry> {
    if crate_root.join("src/lib.rs").is_file() {
        return Some(VerusEntry {
            crate_type: "lib",
            input: PathBuf::from("src/lib.rs"),
        });
    }
    if crate_root.join("src/main.rs").is_file() {
        return Some(VerusEntry {
            crate_type: "bin",
            input: PathBuf::from("src/main.rs"),
        });
    }
    None
}

struct VerusEntry {
    crate_type: &'static str,
    input: PathBuf,
}

#[instrument(level = "debug")]
fn resolve_verus_binary() -> Option<PathBuf> {
    for key in ["CORDIAL_VERUS", "VERUS", "VERUS_PATH"] {
        if let Ok(value) = std::env::var(key) {
            let path = PathBuf::from(value);
            if path.is_file() {
                return Some(path);
            }
        }
    }
    which_verus()
}

#[instrument(level = "debug")]
fn which_verus() -> Option<PathBuf> {
    let Ok(path) = std::env::var("PATH") else {
        return None;
    };
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join("verus");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

#[instrument(level = "info", skip(verus, entry), err(level = "warn"))]
fn run_verus(verus: &Path, crate_root: &Path, entry: &VerusEntry) -> CordialResult<String> {
    let output = Command::new(verus)
        .current_dir(crate_root)
        .arg(format!("--crate-type={}", entry.crate_type))
        .arg(&entry.input)
        .output()?;
    let mut text = String::new();
    text.push_str(&String::from_utf8_lossy(&output.stdout));
    if !output.stderr.is_empty() {
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(&String::from_utf8_lossy(&output.stderr));
    }
    Ok(text)
}

/// Parse rustc-style Verus diagnostics. Summary lines (`N warnings emitted`)
/// are dropped; the same span+message is kept once.
#[instrument(level = "debug", skip(output))]
pub fn parse_verus_compiler_output(output: &str, crate_root: &Path) -> Vec<VerusWarningRecord> {
    let mut records = Vec::new();
    let mut seen = BTreeSet::new();
    let lines: Vec<&str> = output.lines().collect();
    let mut index = 0;
    while index < lines.len() {
        let Some(message) = warning_message(lines[index]) else {
            index += 1;
            continue;
        };
        let Some((file, line)) = span_after(&lines, index + 1) else {
            index += 1;
            continue;
        };
        let key = (file.clone(), line, message.clone());
        if !seen.insert(key) {
            index += 1;
            continue;
        }
        let resolved = resolve_diagnostic_file(crate_root, &file);
        records.push(VerusWarningRecord {
            rule_id: VerusWarningRuleId::Warning001,
            context: file.clone(),
            file: resolved,
            line,
            snippet: truncate_snippet(&message, 96),
        });
        index += 1;
    }
    records.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then(a.line.cmp(&b.line))
            .then(a.snippet.cmp(&b.snippet))
    });
    records
}

#[instrument(level = "trace")]
fn warning_message(line: &str) -> Option<String> {
    let rest = line.strip_prefix("warning:")?;
    let message = rest.trim();
    if is_warning_summary(message) {
        return None;
    }
    if message.is_empty() {
        return None;
    }
    Some(message.to_string())
}

#[instrument(level = "trace")]
fn is_warning_summary(message: &str) -> bool {
    let Some((count, tail)) = message.split_once(' ') else {
        return false;
    };
    if !count.chars().all(|ch| ch.is_ascii_digit()) {
        return false;
    }
    matches!(tail, "warning emitted" | "warnings emitted")
}

#[instrument(level = "trace", skip(lines))]
fn span_after(lines: &[&str], start: usize) -> Option<(String, u32)> {
    for line in lines.iter().skip(start).take(8) {
        if let Some(span) = parse_arrow_span(line) {
            return Some(span);
        }
        if line.starts_with("warning:") || line.starts_with("error:") {
            break;
        }
    }
    None
}

#[instrument(level = "trace")]
fn parse_arrow_span(line: &str) -> Option<(String, u32)> {
    let rest = line.trim().strip_prefix("--> ")?;
    let mut parts = rest.rsplitn(3, ':');
    let _column = parts.next()?;
    let line_no = parts.next()?.parse::<u32>().ok()?;
    let file = parts.next()?.to_string();
    if file.is_empty() {
        return None;
    }
    Some((file, line_no))
}

#[instrument(level = "debug")]
fn resolve_diagnostic_file(crate_root: &Path, file: &str) -> PathBuf {
    let path = PathBuf::from(file);
    if path.is_absolute() {
        return path;
    }
    crate_root.join(path)
}

#[instrument(level = "trace")]
fn truncate_snippet(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max).collect();
    format!("{truncated}…")
}

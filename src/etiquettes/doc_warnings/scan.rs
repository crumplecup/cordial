//! Invoke `cargo doc` and parse `rustdoc::*` diagnostics.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::DocWarningsThresholds;
use crate::error::CordialResult;

use super::types::{DocWarningRecord, DocWarningRuleId};

use tracing::instrument;

/// Scan a crate: run `cargo doc` unless this package is skipped or cargo
/// is not on `PATH`.
///
/// `resolve_root` -- the base a reported diagnostic file path is joined
/// against -- is deliberately a separate parameter from `crate_root` (the
/// literal directory `cargo doc` is invoked in, via `current_dir` and
/// `-p`): confirmed the hard way that these differ in a real multi-member
/// workspace. Cargo's own JSON diagnostics report `file_name` relative to
/// the *workspace* root even when invoked with `current_dir` set to one
/// member's own directory -- joining against `crate_root` there
/// double-prepends the member's own relative path (`crates/amenable/
/// crates/amenable/src/...`), confirmed against a real `cargo doc` run,
/// not assumed. Pass the project/workspace root here; for a standalone
/// (non-workspace) crate the two are the same path, so this is a no-op
/// there.
#[instrument(level = "debug", skip(policy), err(level = "warn"))]
pub fn scan_crate_doc_warnings(
    crate_root: &Path,
    resolve_root: &Path,
    crate_name: &str,
    policy: &DocWarningsThresholds,
) -> CordialResult<Vec<DocWarningRecord>> {
    if policy.skip(crate_name) {
        return Ok(Vec::new());
    }
    if !crate_root.join("Cargo.toml").is_file() {
        return Ok(Vec::new());
    }
    let Some(cargo) = resolve_cargo_binary() else {
        tracing::debug!(
            crate_root = %crate_root.display(),
            "cargo binary not found; skipping rustdoc warning scan"
        );
        return Ok(Vec::new());
    };
    let output = run_cargo_doc(&cargo, crate_root, crate_name, policy)?;
    Ok(parse_doc_compiler_output(&output, resolve_root))
}

/// Parse cargo JSON and rustc-style rustdoc diagnostics. rustc lints
/// (including `missing_docs`) are dropped; the same span+message is kept
/// once. `resolve_root` is the base a relative diagnostic file path is
/// joined against -- see [`scan_crate_doc_warnings`]'s own doc comment
/// for why that's the workspace root, not necessarily the scanned
/// crate's own root.
#[instrument(level = "debug", skip(output))]
pub fn parse_doc_compiler_output(output: &str, resolve_root: &Path) -> Vec<DocWarningRecord> {
    let mut records = Vec::new();
    let mut seen = BTreeSet::new();
    let lines: Vec<&str> = output.lines().collect();
    let mut index = 0;
    while index < lines.len() {
        if let Some((lint, file, line, message)) = json_rustdoc_diagnostic(lines[index]) {
            push_record(
                &mut records,
                &mut seen,
                resolve_root,
                lint,
                file,
                line,
                message,
            );
            index += 1;
            continue;
        }
        let Some((lint, message)) = rustdoc_lint_message(lines[index]) else {
            index += 1;
            continue;
        };
        let Some((file, line)) = span_after(&lines, index + 1) else {
            index += 1;
            continue;
        };
        push_record(
            &mut records,
            &mut seen,
            resolve_root,
            lint,
            file,
            line,
            message,
        );
        index += 1;
    }
    records.sort_by(|left, right| {
        left.file
            .cmp(&right.file)
            .then(left.line.cmp(&right.line))
            .then(left.snippet.cmp(&right.snippet))
            .then(left.context.cmp(&right.context))
    });
    records
}

#[instrument(level = "trace")]
fn json_rustdoc_diagnostic(line: &str) -> Option<(String, String, u32, String)> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    if value.get("reason")?.as_str()? != "compiler-message" {
        return None;
    }
    let message = value.get("message")?;
    let lint = message.get("code")?.get("code")?.as_str()?;
    if !lint.starts_with("rustdoc::") {
        return None;
    }
    let text = message.get("message")?.as_str()?.trim().to_string();
    if text.is_empty() {
        return None;
    }
    let spans = message.get("spans")?.as_array()?;
    let primary = spans
        .iter()
        .find(|span| span.get("is_primary").and_then(serde_json::Value::as_bool) == Some(true))
        .or_else(|| spans.first())?;
    let file = primary.get("file_name")?.as_str()?.to_string();
    if file.is_empty() {
        return None;
    }
    let line = u32::try_from(primary.get("line_start")?.as_u64()?).ok()?;
    Some((lint.to_string(), file, line, text))
}

#[instrument(level = "trace")]
fn rustdoc_lint_message(line: &str) -> Option<(String, String)> {
    let rest = line
        .strip_prefix("warning")
        .or_else(|| line.strip_prefix("error"))?;
    let rest = rest.strip_prefix('[')?;
    let (lint, rest) = rest.split_once(']')?;
    if !lint.starts_with("rustdoc::") {
        return None;
    }
    let message = rest.strip_prefix(':')?.trim();
    if message.is_empty() {
        return None;
    }
    Some((lint.to_string(), message.to_string()))
}

#[instrument(level = "trace", skip(lines))]
fn span_after(lines: &[&str], start: usize) -> Option<(String, u32)> {
    for line in lines.iter().skip(start).take(8) {
        if let Some(span) = parse_arrow_span(line) {
            return Some(span);
        }
        if line.starts_with("warning") || line.starts_with("error") {
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

#[instrument(
    level = "trace",
    skip(records, seen, resolve_root, lint, file, line, message)
)]
fn push_record(
    records: &mut Vec<DocWarningRecord>,
    seen: &mut BTreeSet<(String, u32, String)>,
    resolve_root: &Path,
    lint: String,
    file: String,
    line: u32,
    message: String,
) {
    let key = (file.clone(), line, message.clone());
    if !seen.insert(key) {
        return;
    }
    records.push(DocWarningRecord {
        rule_id: DocWarningRuleId::Warning001,
        context: lint,
        file: resolve_diagnostic_file(resolve_root, &file),
        line,
        snippet: message,
    });
}

#[instrument(level = "debug")]
fn resolve_diagnostic_file(resolve_root: &Path, file: &str) -> PathBuf {
    let path = PathBuf::from(file);
    if path.is_absolute() {
        return path;
    }
    resolve_root.join(path)
}

#[instrument(level = "debug")]
fn resolve_cargo_binary() -> Option<PathBuf> {
    for key in ["CORDIAL_CARGO", "CARGO"] {
        if let Ok(value) = std::env::var(key) {
            let path = PathBuf::from(value);
            if path.is_file() {
                return Some(path);
            }
        }
    }
    which_cargo()
}

#[instrument(level = "debug")]
fn which_cargo() -> Option<PathBuf> {
    let Ok(path) = std::env::var("PATH") else {
        return None;
    };
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join("cargo");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

#[instrument(level = "info", skip(cargo, policy), err(level = "warn"))]
fn run_cargo_doc(
    cargo: &Path,
    crate_root: &Path,
    crate_name: &str,
    policy: &DocWarningsThresholds,
) -> CordialResult<String> {
    let target_dir = crate_root.join("target");
    let mut command = Command::new(cargo);
    command
        .current_dir(crate_root)
        .arg("doc")
        .arg("--no-deps")
        .arg("--message-format=json")
        .arg("-p")
        .arg(crate_name)
        .arg("--target-dir")
        .arg(&target_dir);
    if policy.all_features() {
        command.arg("--all-features");
    }
    if policy.document_private_items() {
        command.arg("--document-private-items");
    }
    let output = command.output()?;
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

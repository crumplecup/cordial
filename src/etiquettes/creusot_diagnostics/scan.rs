//! Invoke `cargo creusot prove` and parse rustc-style diagnostics.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::CreusotDiagnosticsThresholds;
use crate::error::CordialResult;

use super::types::{CreusotDiagnosticRecord, CreusotDiagnosticRuleId};

use tracing::instrument;

/// Scan a crate: run Creusot when this crate is a Creusot target and the
/// project has a Creusot runner available.
#[instrument(level = "debug", skip(policy), err(level = "warn"))]
pub fn scan_crate_creusot_diagnostics(
    crate_root: &Path,
    crate_name: &str,
    policy: &CreusotDiagnosticsThresholds,
) -> CordialResult<Vec<CreusotDiagnosticRecord>> {
    if policy.skip(crate_name) || !crate_is_creusot_target(crate_root) {
        return Ok(Vec::new());
    }
    let Some(runner) = resolve_creusot_runner(crate_name) else {
        tracing::debug!(
            crate_root = %crate_root.display(),
            crate_name,
            "cargo-creusot not found; skipping Creusot diagnostic scan"
        );
        return Ok(Vec::new());
    };
    let output = run_creusot(&runner, crate_root)?;
    parse_creusot_compiler_output(&output.text, crate_root, output.success)
}

/// True when this member is meant to be compiled with Creusot.
#[instrument(level = "debug")]
pub fn crate_is_creusot_target(crate_root: &Path) -> bool {
    if crate_root
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with("_creusot"))
    {
        return true;
    }
    let Ok(manifest) = std::fs::read_to_string(crate_root.join("Cargo.toml")) else {
        return false;
    };
    manifest_names_creusot_target(&manifest)
}

#[instrument(level = "debug", skip(manifest))]
fn manifest_names_creusot_target(manifest: &str) -> bool {
    let mut in_package = false;
    let mut in_deps = false;
    for raw in manifest.lines() {
        let line = raw.trim();
        if line.starts_with('[') {
            in_package = line == "[package]";
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
            if in_package && package_name_is_creusot_target(line) {
                return true;
            }
            continue;
        }
        let key = line
            .split(['=', '.', ' '])
            .next()
            .unwrap_or("")
            .trim()
            .trim_matches('"');
        if matches!(
            key,
            "creusot-std" | "creusot_contracts" | "creusot_contracts_proc"
        ) {
            return true;
        }
    }
    false
}

#[instrument(level = "trace")]
fn package_name_is_creusot_target(line: &str) -> bool {
    let Some((key, value)) = line.split_once('=') else {
        return false;
    };
    key.trim() == "name" && value.trim().trim_matches('"').ends_with("_creusot")
}

#[derive(Debug, Clone)]
struct CreusotRunner {
    program: PathBuf,
    args: Vec<String>,
}

#[derive(Debug, Clone)]
struct CreusotOutput {
    text: String,
    success: bool,
}

#[instrument(level = "debug")]
fn resolve_creusot_runner(crate_name: &str) -> Option<CreusotRunner> {
    if let Ok(value) = std::env::var("CORDIAL_CREUSOT") {
        let path = PathBuf::from(value);
        if path.is_file() {
            return Some(CreusotRunner {
                program: path,
                args: vec![
                    "prove".to_string(),
                    "--".to_string(),
                    "-p".to_string(),
                    crate_name.to_string(),
                ],
            });
        }
    }
    if cargo_creusot_on_path().is_some() {
        return Some(CreusotRunner {
            program: PathBuf::from("cargo"),
            args: vec![
                "creusot".to_string(),
                "prove".to_string(),
                "--".to_string(),
                "-p".to_string(),
                crate_name.to_string(),
            ],
        });
    }
    None
}

#[instrument(level = "debug")]
fn cargo_creusot_on_path() -> Option<PathBuf> {
    let Ok(path) = std::env::var("PATH") else {
        return None;
    };
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join("cargo-creusot");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

#[instrument(level = "info", skip(runner), err(level = "warn"))]
fn run_creusot(runner: &CreusotRunner, crate_root: &Path) -> CordialResult<CreusotOutput> {
    let output = Command::new(&runner.program)
        .current_dir(workspace_root(crate_root))
        .args(&runner.args)
        .output()?;
    let mut text = String::new();
    text.push_str(&String::from_utf8_lossy(&output.stdout));
    if !output.stderr.is_empty() {
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(&String::from_utf8_lossy(&output.stderr));
    }
    Ok(CreusotOutput {
        text,
        success: output.status.success(),
    })
}

#[instrument(level = "debug")]
fn workspace_root(crate_root: &Path) -> PathBuf {
    for ancestor in crate_root.ancestors() {
        let Ok(manifest) = std::fs::read_to_string(ancestor.join("Cargo.toml")) else {
            continue;
        };
        if manifest
            .lines()
            .any(|line| line.trim() == "[workspace]" || line.trim().starts_with("[workspace."))
        {
            return ancestor.to_path_buf();
        }
    }
    crate_root.to_path_buf()
}

/// Parse rustc-style Creusot diagnostics. Summary lines (`N warnings emitted`)
/// are dropped; the same span+severity+message is kept once. A failed verifier
/// run with no parseable `error:` span produces one crate-level failure record.
#[instrument(level = "debug", skip(output), err(level = "warn"))]
pub fn parse_creusot_compiler_output(
    output: &str,
    crate_root: &Path,
    success: bool,
) -> CordialResult<Vec<CreusotDiagnosticRecord>> {
    let mut records = Vec::new();
    let mut seen = BTreeSet::new();
    let lines: Vec<&str> = output.lines().collect();
    let mut index = 0;
    while index < lines.len() {
        let Some((rule_id, message)) = diagnostic_message(lines[index]) else {
            index += 1;
            continue;
        };
        let Some((file, line)) = span_after(&lines, index + 1) else {
            index += 1;
            continue;
        };
        let key = (rule_id, file.clone(), line, message.clone());
        if !seen.insert(key) {
            index += 1;
            continue;
        }
        let resolved = resolve_diagnostic_file(crate_root, &file);
        records.push(
            CreusotDiagnosticRecord::builder()
                .rule_id(rule_id)
                .context(file.clone())
                .file(resolved)
                .line(line)
                .snippet(truncate_snippet(&message, 96))
                .build()?,
        );
        index += 1;
    }
    if !success
        && !records
            .iter()
            .any(|record| record.rule_id() == CreusotDiagnosticRuleId::Failure001)
    {
        records.push(
            CreusotDiagnosticRecord::builder()
                .rule_id(CreusotDiagnosticRuleId::Failure001)
                .context("<crate>".to_string())
                .file(crate_root.join("Cargo.toml"))
                .line(1)
                .snippet(truncate_snippet(first_non_empty_line(output), 96))
                .build()?,
        );
    }
    records.sort_by(|a, b| {
        a.file()
            .cmp(b.file())
            .then(a.line().cmp(&b.line()))
            .then(a.rule_id().as_str().cmp(b.rule_id().as_str()))
            .then(a.snippet().cmp(b.snippet()))
    });
    Ok(records)
}

#[instrument(level = "trace")]
fn diagnostic_message(line: &str) -> Option<(CreusotDiagnosticRuleId, String)> {
    let (rule_id, rest) = if let Some(rest) = line.strip_prefix("warning:") {
        (CreusotDiagnosticRuleId::Warning001, rest)
    } else {
        let rest = line.strip_prefix("error:")?;
        (CreusotDiagnosticRuleId::Failure001, rest)
    };
    let message = rest.trim();
    if is_diagnostic_summary(message) || message.is_empty() {
        return None;
    }
    Some((rule_id, message.to_string()))
}

#[instrument(level = "trace")]
fn is_diagnostic_summary(message: &str) -> bool {
    let Some((count, tail)) = message.split_once(' ') else {
        return false;
    };
    if !count.chars().all(|ch| ch.is_ascii_digit()) {
        return false;
    }
    matches!(
        tail,
        "warning emitted" | "warnings emitted" | "error emitted" | "errors emitted"
    )
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
        return canonical_or_original(path);
    }
    let workspace_candidate = workspace_root(crate_root).join(&path);
    if workspace_candidate.exists() || path.starts_with("crates") {
        return canonical_or_original(workspace_candidate);
    }
    canonical_or_original(crate_root.join(path))
}

#[instrument(level = "trace")]
fn canonical_or_original(path: PathBuf) -> PathBuf {
    std::fs::canonicalize(&path).unwrap_or(path)
}

#[instrument(level = "trace")]
fn first_non_empty_line(output: &str) -> &str {
    output
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("creusot prove failed without a parseable diagnostic")
        .trim()
}

#[instrument(level = "trace")]
fn truncate_snippet(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max).collect();
    format!("{truncated}…")
}

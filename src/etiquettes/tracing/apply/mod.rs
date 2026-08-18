//! Apply `#[instrument]` attributes from a tracing instrument checklist.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use crate::error::{CordialError, CordialResult};
use crate::loader::CrateTarget;
use crate::session::RunAll;
use crate::targets::discover_crate_targets;

mod instrument;
mod parse;

use super::scan::scan_rust_source;
use instrument::{GapApplyOutcome, apply_gap, ensure_use_instrument, recipe_for_gap};

pub use parse::{parse_tracing_instrument_checklist, parse_tracing_instrument_checklist_text};

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

/// Patch source files listed in the tracing instrument checklist.
#[tracing::instrument(skip(project_root, checklist_path, only_crate))]
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

        let source = std::fs::read_to_string(&path)?;
        let src_root = {
            let src = crate_root.join("src");
            if src.is_dir() {
                src
            } else {
                crate_root.clone()
            }
        };
        let extra_skip = crate::config::load_cordial_config(project_root, project_root)
            .tracing
            .extra_skip;
        let records = match scan_rust_source(
            &source,
            &path,
            &src_root,
            crate_root,
            &crate_name,
            &extra_skip,
        ) {
            Ok(records) => records,
            Err(error) => {
                tracing::warn!(
                    path = %path.display(),
                    crate_name = %crate_name,
                    error = %error,
                    "failed to classify functions for apply"
                );
                summary.unresolved += file_gaps.len();
                continue;
            }
        };
        let mut lines: Vec<String> = source.lines().map(str::to_string).collect();
        dedupe_gaps(&mut file_gaps);
        file_gaps.sort_by_key(|right| std::cmp::Reverse(right.line));

        let mut file_changed = false;
        for gap in file_gaps {
            let Some(recipe) = recipe_for_gap(&records, &gap) else {
                tracing::warn!(
                    path = %gap.rel_path.display(),
                    line = gap.line,
                    qualified_name = %gap.qualified_name,
                    "no classified recipe for checklist item"
                );
                summary.unresolved += 1;
                continue;
            };
            match apply_gap(&mut lines, &gap, recipe) {
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

fn dedupe_gaps(gaps: &mut Vec<InstrumentGap>) {
    gaps.sort_by(|left, right| {
        left.qualified_name
            .cmp(&right.qualified_name)
            .then(left.rel_path.cmp(&right.rel_path))
            .then(left.line.cmp(&right.line))
    });
    gaps.dedup_by(|later, earlier| {
        later.qualified_name == earlier.qualified_name
            && later.rel_path == earlier.rel_path
            && later.line == earlier.line
    });
}

//! Apply `#[instrument]` attributes from current tracing findings or a checklist.
//!
//! Selects open findings from the current source IR (or reads `--checklist`)
//! and writes the classified recipe onto the named
//! function. `cordial quality --apply` is the CLI; `--dry-run` logs without
//! writing. Path-qualified forms (`#[tracing::instrument]`) are used when
//! `instrument` would clash with a module name. Unrecordable parameters
//! (`impl Trait`, `dyn`, generics) are skipped as fields; `err` is only
//! applied when the function returns `Result` (or a `*Result` alias)
//! whose `Err` type is known to implement `Display`.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use crate::error::CordialResult;
use crate::loader::{CrateTarget, SourceLoadView};
use crate::session::RunAll;
use crate::targets::discover_crate_targets;

mod instrument;
mod ir;
mod parse;
mod verifier_policy;

use instrument::{
    GapApplyOutcome, InstrumentAttrStyle, apply_gap, attr_style, ensure_use_instrument,
    recipe_for_gap, strip_instrument,
};
use ir::{build_apply_ir, recipes_for_file};

pub use parse::{parse_tracing_instrument_checklist, parse_tracing_instrument_checklist_text};
pub use verifier_policy::TracingApplyPolicy;

pub(super) use verifier_policy::{
    crate_gate_cfgs, gate_attr, gate_predicate, resolve_tracing_apply_policy,
};

/// One open checklist row targeting a function or method.
#[derive(Debug, Clone, PartialEq, Eq, derive_builder::Builder, derive_getters::Getters)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub struct InstrumentGap {
    /// Cargo package name.
    crate_name: String,
    /// Fully qualified item name.
    qualified_name: String,
    /// Path of the patched file relative to the crate root.
    rel_path: PathBuf,
    /// Source line number (1-based), when known.
    #[getter(copy)]
    line: u32,
}

impl InstrumentGap {
    /// Start a builder for this value.
    #[tracing::instrument(level = "debug")]
    pub fn builder() -> InstrumentGapBuilder {
        InstrumentGapBuilder::default()
    }
}

/// Result of applying instrumentation patches.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters)]
pub struct InstrumentApplySummary {
    /// How many functions received `#[instrument]`.
    #[getter(copy)]
    changed_functions: usize,
    /// How many source files were rewritten.
    #[getter(copy)]
    changed_files: usize,
    /// Items that already satisfied the recipe.
    #[getter(copy)]
    skipped_existing: usize,
    /// Left untouched because the real verifier toolchain for every
    /// crate that compiles the file can't tolerate `#[instrument]`,
    /// gated or not (apply skip policy).
    #[getter(copy)]
    skipped_policy: usize,
    /// Checklist rows that could not be matched in source.
    #[getter(copy)]
    unresolved: usize,
    /// Functions whose source edits were successfully written.
    applied_gaps: Vec<InstrumentGap>,
}

/// Patch source files listed in the tracing instrument checklist.
#[tracing::instrument(level = "info", err(level = "warn"))]
pub fn run_tracing_instrument_apply(
    project_root: &Path,
    checklist_path: &Path,
    only_crate: Option<&str>,
    dry_run: bool,
) -> CordialResult<InstrumentApplySummary> {
    let store_home = crate::default_store_home();
    run_apply(
        project_root,
        &store_home,
        ApplyTargets::Checklist(checklist_path),
        only_crate,
        dry_run,
    )
}

/// Source of the functions selected for tracing apply.
#[derive(Clone, Copy)]
pub(crate) enum ApplyTargets<'a> {
    Checklist(&'a Path),
    #[cfg(feature = "cli")]
    CurrentIr(&'a crate::store::StoreLayout),
}

impl<'a> ApplyTargets<'a> {
    #[tracing::instrument(level = "trace", skip(self))]
    fn checklist_path(self) -> Option<&'a Path> {
        match self {
            Self::Checklist(path) => Some(path),
            #[cfg(feature = "cli")]
            Self::CurrentIr(_) => None,
        }
    }
}

/// Apply the selected tracing recipes using the configured store home.
#[tracing::instrument(level = "debug", skip(targets_mode), err(level = "warn"))]
pub(crate) fn run_apply(
    project_root: &Path,
    store_home: &Path,
    targets_mode: ApplyTargets<'_>,
    only_crate: Option<&str>,
    dry_run: bool,
) -> CordialResult<InstrumentApplySummary> {
    let filter = RunAll;
    let mut targets = discover_crate_targets(project_root, &filter)?;
    if let Some(name) = only_crate {
        targets.retain(|target| target.crate_name() == name);
    }
    let crate_roots: HashMap<String, PathBuf> = targets
        .iter()
        .map(|target: &CrateTarget| (target.crate_name().clone(), target.crate_root().clone()))
        .collect();

    let mut by_file: BTreeMap<(String, PathBuf), Vec<InstrumentGap>> = BTreeMap::new();
    let checklist_path = targets_mode.checklist_path();
    if let Some(path) = checklist_path {
        for gap in parse_tracing_instrument_checklist(path)? {
            if !crate_roots.contains_key(gap.crate_name()) {
                tracing::warn!(
                    crate_name = %gap.crate_name(),
                    "skipping gap for crate not in scan targets"
                );
                continue;
            }
            by_file
                .entry((gap.crate_name().clone(), gap.rel_path().clone()))
                .or_default()
                .push(gap);
        }
    }

    let mut summary = InstrumentApplySummary {
        changed_functions: 0,
        changed_files: 0,
        skipped_existing: 0,
        skipped_policy: 0,
        unresolved: 0,
        applied_gaps: Vec::new(),
    };
    if checklist_path.is_some() && by_file.is_empty() {
        tracing::info!("no open checklist items found in tracing instrument checklist");
        return Ok(summary);
    }

    let _parsed_source_session = crate::loader::ParsedSourceSession::begin();
    let source_view = SourceLoadView::for_standalone_scan();
    let config = crate::config::load_cordial_config(project_root, store_home);
    let selected_files = checklist_path.map(|_| &by_file);
    let workspace = build_apply_ir(project_root, store_home, &targets, selected_files)?;
    #[cfg(feature = "cli")]
    if let ApplyTargets::CurrentIr(store) = targets_mode {
        for gap in ir::gaps_from_ir(project_root, store_home, &workspace, &targets, store)? {
            by_file
                .entry((gap.crate_name().clone(), gap.rel_path().clone()))
                .or_default()
                .push(gap);
        }
    }
    if by_file.is_empty() {
        tracing::info!("no open tracing items to apply");
        return Ok(summary);
    }
    let facts = workspace.path_inclusions().ok_or_else(|| {
        crate::error::CordialError::workspace_source_facts_missing("path inclusions")
    })?;
    let call_graph = workspace
        .call_graph()
        .ok_or_else(|| crate::error::CordialError::workspace_source_facts_missing("call graph"))?;

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

        let parsed = source_view.parsed_file(&path, "tracing-instrument-apply")?;
        let source = parsed.source();
        let never_instrument = call_graph.never_instrument(&crate_name);
        let records = match recipes_for_file(&workspace, &crate_name, &rel_path) {
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
        file_gaps.sort_by_key(|right| std::cmp::Reverse(right.line()));
        let style = attr_style(&lines);
        let policy =
            resolve_tracing_apply_policy(&crate_name, &path, crate_root, config.tracing(), &facts);
        if policy == TracingApplyPolicy::Skip {
            tracing::info!(
                path = %path.display(),
                crate_name = %crate_name,
                "skip-policy file: apply strips existing #[instrument], never writes one"
            );
        }

        let mut file_changed = false;
        let mut file_applied = Vec::new();
        for gap in file_gaps {
            let strip = never_instrument.contains(gap.qualified_name())
                || policy == TracingApplyPolicy::Skip;
            if strip {
                match strip_instrument(&mut lines, &gap) {
                    GapApplyOutcome::Applied => {
                        summary.changed_functions += 1;
                        file_changed = true;
                        file_applied.push(gap.clone());
                    }
                    GapApplyOutcome::AlreadyInstrumented => {
                        summary.skipped_existing += 1;
                    }
                    GapApplyOutcome::Unresolved => {
                        summary.unresolved += 1;
                    }
                    GapApplyOutcome::SkippedPolicy => {
                        summary.skipped_policy += 1;
                    }
                }
                continue;
            }
            let Some(recipe) = recipe_for_gap(&records, &gap) else {
                tracing::warn!(
                    path = %gap.rel_path().display(),
                    line = gap.line(),
                    qualified_name = %gap.qualified_name(),
                    "no classified recipe for checklist item"
                );
                summary.unresolved += 1;
                continue;
            };
            match apply_gap(&mut lines, &gap, recipe, style, &policy)? {
                GapApplyOutcome::Applied => {
                    summary.changed_functions += 1;
                    file_changed = true;
                    file_applied.push(gap.clone());
                }
                GapApplyOutcome::AlreadyInstrumented => {
                    summary.skipped_existing += 1;
                }
                GapApplyOutcome::Unresolved => {
                    summary.unresolved += 1;
                }
                GapApplyOutcome::SkippedPolicy => {
                    summary.skipped_policy += 1;
                }
            }
        }

        if file_changed {
            // Only `Bare` ever writes the short `instrument(..)` form
            // that needs this import -- `Gated` always renders fully
            // qualified (see `apply_gap`), so inserting it regardless
            // of `policy` would leave a real `unused_imports` warning
            // in any file where every applied gap was `Gated`.
            if style == InstrumentAttrStyle::Short && policy == TracingApplyPolicy::Bare {
                lines = ensure_use_instrument(lines);
            }
            if dry_run {
                tracing::info!(path = %path.display(), "dry run: would update file");
            } else {
                std::fs::write(&path, format!("{}\n", lines.join("\n")))?;
                summary.applied_gaps.extend(file_applied);
            }
            summary.changed_files += 1;
        }
    }

    tracing::info!(
        changed_functions = summary.changed_functions(),
        changed_files = summary.changed_files(),
        skipped_existing = summary.skipped_existing(),
        skipped_policy = summary.skipped_policy(),
        unresolved = summary.unresolved(),
        dry_run,
        "tracing instrument apply complete"
    );
    Ok(summary)
}

#[tracing::instrument(level = "debug", skip(gaps))]
fn dedupe_gaps(gaps: &mut Vec<InstrumentGap>) {
    gaps.sort_by(|left, right| {
        left.qualified_name()
            .cmp(right.qualified_name())
            .then(left.rel_path().cmp(right.rel_path()))
            .then(left.line().cmp(&right.line()))
    });
    gaps.dedup_by(|later, earlier| {
        later.qualified_name() == earlier.qualified_name()
            && later.rel_path() == earlier.rel_path()
            && later.line() == earlier.line()
    });
}

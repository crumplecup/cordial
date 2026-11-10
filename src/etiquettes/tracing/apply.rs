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
mod lint_policy;
mod parse;
mod plan;
mod preserve;
mod verifier_policy;

use ir::build_apply_ir;

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
    /// Functions with validated edits proposed by a dry run; never written or claimed.
    proposed_gaps: Vec<InstrumentGap>,
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
        proposed_gaps: Vec::new(),
    };
    if checklist_path.is_some() && by_file.is_empty() {
        tracing::info!("no open checklist items found in tracing instrument checklist");
        return Ok(summary);
    }

    let config = crate::config::load_cordial_config(project_root, store_home);
    if !config.etiquette_may_run("tracing") {
        return Ok(summary);
    }
    let parsed_source_session = crate::loader::ParsedSourceSession::begin_in(project_root)?;
    let source_view = SourceLoadView::for_session();
    let selected_files = checklist_path.map(|_| &by_file);
    let workspace = build_apply_ir(
        project_root,
        store_home,
        &parsed_source_session,
        &targets,
        selected_files,
    )?;
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

    let mut planned = Vec::new();
    let mut applied = Vec::new();

    let planner = plan::FilePlanner::new(
        project_root,
        &source_view,
        &workspace,
        &config,
        facts,
        call_graph,
    );
    for ((crate_name, rel_path), file_gaps) in by_file {
        let Some(crate_root) = crate_roots.get(&crate_name) else {
            continue;
        };
        if let Some((write, file_applied)) =
            planner.plan_file(&crate_name, crate_root, &rel_path, file_gaps, &mut summary)?
        {
            planned.push(write);
            if dry_run {
                tracing::info!(path = %crate_root.join(&rel_path).display(), "dry run: would update file");
            }
            applied.extend(file_applied);
        }
    }

    crate::source_write::write_sources(&planned, dry_run, || Ok(()))?;
    if dry_run {
        summary.proposed_gaps = applied;
    } else {
        summary.applied_gaps = applied;
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

//! Conservative string field storage edits for the selected error strategy.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::config::ErrorFieldStrategy;
use crate::config::load_cordial_config;
use crate::error::CordialResult;
use crate::ir::{IrMut, IrView};
use crate::loader::{SourceEditView, SourceLoadView};
use crate::session::RunAll;
use crate::targets::discover_crate_targets;

mod getter_uses;
pub(super) mod getters;
mod graph_apply;
mod initializers;
mod inventory;
mod lifetime;
mod names;
mod patch;
mod write;

pub(crate) use self::getter_uses::GetterConsumerFacts;
pub(crate) use self::getters::GetterContractFacts;
pub(crate) use self::graph_apply::{GraphDeclarationFacts, GraphExprFacts, GraphLifetimeFacts};
use self::graph_apply::{conversion_graphs_from_ir, enrich_conversion_graphs};
pub(crate) use self::initializers::InitializerFileFacts;
pub(crate) use self::inventory::{
    ConversionFileFacts, conversion_units_from_ir, enrich_conversion_units,
};
pub use self::inventory::{
    ErrorConversionSite, ErrorConversionSiteKind, ErrorConversionUnit, ErrorEdgeEvidence,
    ErrorEdgeField, ErrorEdgeStorage, ErrorEdgeTarget, ErrorEdgeTargetKind, ErrorFromEvidence,
    ErrorSourceMethodEvidence, ErrorTryEvidence, ErrorTryScope,
};
pub(crate) use self::lifetime::LifetimeFileFacts;
pub(crate) use self::names::NameAudit;
use self::patch::patch_file;
use self::write::write_planned_files;

/// A compliance site repaired by a written edit, or proposed by a dry run.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters)]
pub struct ErrorFieldApplyRepair {
    crate_name: String,
    file: PathBuf,
    context: String,
    #[getter(copy)]
    line: u32,
    #[getter(copy)]
    rule_id: super::InternalErrorComplianceId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Edit {
    start: usize,
    end: usize,
    replacement: String,
}

struct FilePlan {
    patched: Option<String>,
    converted: usize,
    blockers: Vec<ErrorFieldApplyBlocker>,
    units: Vec<ErrorConversionUnit>,
    edits: Vec<Edit>,
}

/// Why an error field could not be converted safely.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorFieldApplyBlockerReason {
    /// The type or field is public, duplicated, or used in another file.
    OutsideFile,
    /// A local definition, import, or alias makes a relied-on name ambiguous.
    AmbiguousName,
    /// At least one initializer is missing or outside the recognized forms.
    Initializer,
    /// A field use may require the existing storage type.
    FieldUse,
    /// Source spans or planned edits could not be used safely.
    InvalidEdit,
    /// The connected parent, Kind, source, constructor, and use graph is incomplete.
    GraphClosure,
    /// A required part of the atomic conversion is disabled by lint policy.
    DisabledPolicy,
}

/// A candidate left untouched by the error field apply pass.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters)]
pub struct ErrorFieldApplyBlocker {
    /// File containing the candidate field.
    path: PathBuf,
    /// Error struct containing the field.
    type_name: String,
    /// Candidate field name.
    field_name: String,
    /// Why the field was left unchanged.
    #[getter(copy)]
    reason: ErrorFieldApplyBlockerReason,
}

impl ErrorFieldApplyBlocker {
    #[instrument(level = "debug", skip(path, reason))]
    fn new(
        path: &Path,
        type_name: &str,
        field_name: &str,
        reason: ErrorFieldApplyBlockerReason,
    ) -> Self {
        Self {
            path: path.to_path_buf(),
            type_name: type_name.to_string(),
            field_name: field_name.to_string(),
            reason,
        }
    }
}

/// Result of changing string fields and their recognized initializers.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters)]
pub struct ErrorFieldApplySummary {
    /// Source files changed, or that would change under `--dry-run`.
    #[getter(copy)]
    changed_files: usize,
    /// Fields whose type and every initializer were converted.
    #[getter(copy)]
    converted_fields: usize,
    /// Candidate fields left untouched because the conversion was incomplete.
    #[getter(copy)]
    unresolved: usize,
    /// Candidate fields left untouched, with their reasons.
    blockers: Vec<ErrorFieldApplyBlocker>,
    /// Construction, use, and unresolved paths for each candidate field.
    conversion_units: Vec<ErrorConversionUnit>,
    /// Exact original compliance sites covered by successfully planned edits.
    /// Dry runs propose these repairs without claiming them on the worklist.
    repairs: Vec<ErrorFieldApplyRepair>,
}

/// Compatibility name for the original file-only apply result.
pub use ErrorFieldApplySummary as ErrorFileApplySummary;

/// Add the strategy-independent conversion facts and the selected graph plan
/// to the same crate IR used by the normal error scan and by apply.
#[instrument(level = "debug", skip(ir, source, strategy), err(level = "warn"))]
pub(crate) fn enrich_error_apply_evidence(
    ir: &mut dyn IrMut,
    source: &SourceLoadView,
    strategy: ErrorFieldStrategy,
) -> CordialResult<()> {
    let files = edit_files(ir, source)?;
    enrich_conversion_units(ir)?;
    enrich_conversion_graphs(ir, &files, strategy)
}

/// Load only the text for the exact files represented by required IR evidence.
#[instrument(level = "debug", skip(ir, source), err(level = "warn"))]
fn edit_files(ir: &dyn IrView, source: &SourceLoadView) -> CordialResult<Vec<SourceEditView>> {
    ir.source_error_apply()?
        .files()
        .iter()
        .map(|file| source.edit_file(file.snapshot()))
        .collect()
}

/// Convert simple string fields to the configured owned or borrowed form.
///
/// Private error structs and fields with recognized initializers are patched:
/// `file` from `Location::caller().file()`, and other
/// string fields from literals, qualified same-module or block-scoped string
/// constants and statics, qualified built-in static string macros, explicit
/// static string constructor parameters, or typed static string local aliases.
/// Parameters tied to an existing private error lifetime can also be borrowed.
/// A closed single-field unit can move one constructor lifetime onto the error
/// type and direct forwarding return types. Wider lifetime propagation and
/// cross-file lifetime introduction is supported for a closed private graph
/// with a named constructor anchor and proven origins. More complex caller
/// constraints remain manual work.
#[instrument(level = "info", err(level = "warn"))]
pub fn run_error_field_apply(
    project_root: &Path,
    store_home: &Path,
    only_crate: Option<&str>,
    dry_run: bool,
) -> CordialResult<ErrorFieldApplySummary> {
    let config = load_cordial_config(project_root, store_home);
    let policy = config.internal_error_chain();
    let mut summary = ErrorFieldApplySummary {
        changed_files: 0,
        converted_fields: 0,
        unresolved: 0,
        blockers: Vec::new(),
        conversion_units: Vec::new(),
        repairs: Vec::new(),
    };
    if !config.etiquette_may_run("internal_error_chain") {
        return Ok(summary);
    }
    let parsed = crate::loader::ParsedSourceSession::begin_in(project_root)?;
    for target in discover_crate_targets(project_root, &RunAll)? {
        if only_crate.is_some_and(|name| name != target.crate_name()) {
            continue;
        }
        let source = SourceLoadView::for_target(&target)?;
        let mut ir = parsed.project_target(&target)?;
        enrich_error_apply_evidence(&mut ir, &source, policy.field_strategy())?;
        let units = conversion_units_from_ir(&ir)?;
        let graphs = conversion_graphs_from_ir(&ir)?;
        let index = inventory::CrateTypeIndex::from_ir(&ir)?;
        let edit_files = edit_files(&ir, &source)?;
        let mut results = Vec::new();
        for file in &edit_files {
            let plan = patch_file(file, policy.field_strategy(), &units, &graphs, dry_run)?;
            results.push((file, plan));
        }
        let disabled: BTreeSet<_> = edit_files
            .iter()
            .filter(|file| {
                let location = crate::LintLocation::new(
                    project_root,
                    Some(target.crate_root()),
                    target.crate_name(),
                    Some(file.path()),
                );
                // These edits change field storage, source locations, and the Kind
                // edge together. A disabled obligation blocks the whole connected
                // transaction rather than authorizing a partial graph rewrite.
                [
                    super::InternalErrorComplianceId::FieldStrategy001,
                    super::InternalErrorComplianceId::SourceShape001,
                    super::InternalErrorComplianceId::ArchKindBox001,
                ]
                .into_iter()
                .any(|rule| !config.lint_enabled("internal_error_chain", rule.as_str(), location))
            })
            .map(|file| file.path().to_path_buf())
            .collect();
        let mut rejected = disabled.clone();
        loop {
            let before = rejected.len();
            for graph in &graphs {
                if graph.component_paths().is_empty() {
                    continue;
                }
                let incomplete = graph.component_paths().iter().any(|path| {
                    rejected.contains(path)
                        || !results
                            .iter()
                            .any(|(file, plan)| file.path() == path && plan.patched.is_some())
                });
                if incomplete {
                    rejected.extend(graph.component_paths().iter().cloned());
                }
            }
            if rejected.len() == before {
                break;
            }
        }
        let mut planned = Vec::new();
        let mut repairs = Vec::new();
        for (file, plan) in results {
            let FilePlan {
                mut patched,
                mut converted,
                mut blockers,
                units,
                edits,
            } = plan;
            if rejected.contains(file.path()) {
                patched = None;
                converted = 0;
                if let Some(graph) = graphs.iter().find(|graph| graph.path() == file.path()) {
                    for (type_name, field_name) in graph.fields() {
                        if !blockers.iter().any(|blocker| {
                            blocker.type_name() == type_name && blocker.field_name() == field_name
                        }) {
                            blockers.push(ErrorFieldApplyBlocker::new(
                                file.path(),
                                type_name,
                                field_name,
                                if disabled.contains(file.path()) {
                                    ErrorFieldApplyBlockerReason::DisabledPolicy
                                } else {
                                    ErrorFieldApplyBlockerReason::GraphClosure
                                },
                            ));
                        }
                    }
                }
            }
            summary.conversion_units.extend(units);
            summary.converted_fields += converted;
            summary.unresolved += blockers.len();
            for blocker in &blockers {
                tracing::info!(path = %blocker.path.display(), type_name = %blocker.type_name, field = %blocker.field_name, reason = ?blocker.reason, "error field apply: candidate left unchanged");
            }
            summary.blockers.extend(blockers);
            if let Some(patched) = patched {
                let facts = index
                    .graph_declarations(file)
                    .ok_or_else(|| crate::CordialError::stale_source_snapshot(file.path()))?;
                repairs.extend(facts.repairs(
                    target.crate_name(),
                    target.crate_root(),
                    file,
                    &edits,
                    policy.field_strategy(),
                ));
                planned.push((file, patched, converted));
            }
        }
        write_planned_files(&planned, dry_run)?;
        summary.changed_files += planned.len();
        summary.repairs.extend(repairs);
    }
    Ok(summary)
}

/// Compatibility name for the original file-only apply operation.
pub use run_error_field_apply as run_error_file_apply;

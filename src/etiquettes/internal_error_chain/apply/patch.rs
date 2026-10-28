//! Plan and validate edits for one shared parsed source file.

use tracing::instrument;

use crate::config::ErrorFieldStrategy;
use crate::error::{CordialError, CordialResult};
use crate::loader::SourceEditView;

use super::graph_apply::ErrorGraphPlan;
use super::initializers::FieldKind;
use super::inventory::StringStorage;
use super::{
    Edit, ErrorConversionUnit, ErrorFieldApplyBlocker, ErrorFieldApplyBlockerReason, FilePlan,
};

#[instrument(level = "debug", skip(file, strategy, ir_units, graphs))]
pub(super) fn patch_file(
    file: &SourceEditView,
    strategy: ErrorFieldStrategy,
    ir_units: &[ErrorConversionUnit],
    graphs: &[ErrorGraphPlan],
    dry_run: bool,
) -> CordialResult<FilePlan> {
    let mut edits = Vec::new();
    let mut converted = 0;
    let mut blockers = Vec::new();
    let mut pending = Vec::new();
    let mut units = Vec::new();
    let graph = graphs.iter().find(|graph| graph.path() == file.path());
    if let Some(graph) = graph {
        if graph.source_digest() != file.source_digest() {
            return Err(CordialError::stale_source_snapshot(file.path()));
        }
        for (type_name, field_name) in graph.fields() {
            if graph.edits().is_none() {
                blockers.push(ErrorFieldApplyBlocker::new(
                    file.path(),
                    type_name,
                    field_name,
                    ErrorFieldApplyBlockerReason::GraphClosure,
                ));
            }
        }
        if let Some(graph_edits) = graph.edits() {
            edits.extend(graph_edits.iter().cloned());
            converted += graph.fields().len();
            pending.extend(graph.fields().iter().cloned());
        }
    }
    for unit in ir_units
        .iter()
        .filter(|unit| unit.declaration() == file.path())
    {
        if graph.is_some_and(|graph| graph.covers(unit.type_name())) {
            units.push(unit.clone());
            continue;
        }
        if unit.source_digest() != file.source_digest() {
            return Err(CordialError::stale_source_snapshot(file.path()));
        }
        let unit = unit.clone();
        let name = unit.type_name().clone();
        let field_name = unit.field_name().clone();
        let field = unit.field_evidence();
        let call_site = field.kind() == FieldKind::CallSiteFile && field.private_field();
        let has_cross_file_reference = unit.sites().iter().any(|site| site.path() != file.path());
        let has_unknown_field_use = if call_site {
            unit.blocks_call_site_edit()
        } else {
            unit.blocks_existing_field_edit()
        };
        let name_ambiguous = unit.name_ambiguous();
        let lifetime_candidate = unit.lifetime_candidate(strategy);
        if lifetime_candidate && !has_cross_file_reference && !name_ambiguous && field.private() {
            if let Some(mut planned) = unit.lifetime_edits(file.source(), strategy) {
                edits.append(&mut planned);
                pending.push((name.clone(), field_name));
                converted += 1;
                units.push(unit);
                continue;
            }
        }
        if lifetime_candidate && strategy == ErrorFieldStrategy::Owned {
            blockers.push(ErrorFieldApplyBlocker::new(
                file.path(),
                &name,
                &field_name,
                ErrorFieldApplyBlockerReason::Initializer,
            ));
            units.push(unit);
            continue;
        }
        let source_type = match strategy {
            ErrorFieldStrategy::Borrowed if matches!(field.storage(), StringStorage::Owned) => {
                let lifetime = if call_site {
                    None
                } else {
                    field.struct_lifetime().as_deref()
                };
                format!("&'{} str", lifetime.unwrap_or("static"))
            }
            ErrorFieldStrategy::Owned if matches!(field.storage(), StringStorage::Borrowed { lifetime: Some(lifetime), mutable: false } if lifetime == "static") => {
                "String".to_string()
            }
            ErrorFieldStrategy::Owned if matches!(field.storage(), StringStorage::Borrowed { lifetime: Some(lifetime), mutable: false } if field.struct_lifetime().as_deref() == Some(lifetime.as_str())) => {
                "String".to_string()
            }
            _ => continue,
        };
        let initializer_edits = unit.initializer_edits(file.source(), strategy);
        units.push(unit.clone());
        if !field.unique_struct() || (!call_site && (!field.private() || has_cross_file_reference))
        {
            blockers.push(ErrorFieldApplyBlocker::new(
                file.path(),
                &name,
                &field_name,
                ErrorFieldApplyBlockerReason::OutsideFile,
            ));
            continue;
        }
        if name_ambiguous {
            blockers.push(ErrorFieldApplyBlocker::new(
                file.path(),
                &name,
                &field_name,
                ErrorFieldApplyBlockerReason::AmbiguousName,
            ));
            continue;
        }
        let Some(mut initializers) = initializer_edits else {
            blockers.push(ErrorFieldApplyBlocker::new(
                file.path(),
                &name,
                &field_name,
                ErrorFieldApplyBlockerReason::Initializer,
            ));
            continue;
        };
        if has_unknown_field_use {
            blockers.push(ErrorFieldApplyBlocker::new(
                file.path(),
                &name,
                &field_name,
                ErrorFieldApplyBlockerReason::FieldUse,
            ));
            continue;
        }
        let Some(field_span) = field.type_range() else {
            blockers.push(ErrorFieldApplyBlocker::new(
                file.path(),
                &name,
                &field_name,
                ErrorFieldApplyBlockerReason::InvalidEdit,
            ));
            continue;
        };
        edits.push(Edit {
            start: field_span.0,
            end: field_span.1,
            replacement: source_type,
        });
        edits.append(&mut initializers);
        pending.push((name.clone(), field_name));
        converted += 1;
    }
    apply_planned_edits(file, edits, converted, blockers, pending, units, dry_run)
}

#[instrument(level = "debug", skip(file, edits, blockers, pending, units))]
fn apply_planned_edits(
    file: &SourceEditView,
    mut edits: Vec<Edit>,
    converted: usize,
    mut blockers: Vec<ErrorFieldApplyBlocker>,
    pending: Vec<(String, String)>,
    units: Vec<ErrorConversionUnit>,
    dry_run: bool,
) -> CordialResult<FilePlan> {
    if edits.is_empty() {
        return Ok(FilePlan {
            patched: None,
            converted,
            blockers,
            units,
            edits,
        });
    }
    edits.sort_by_key(|edit| std::cmp::Reverse(edit.start));
    let mut patched = file.source().to_string();
    let mut limit = patched.len();
    for edit in &edits {
        if edit.end > limit
            || edit.start > edit.end
            || file.source().get(edit.start..edit.end).is_none()
        {
            blockers.extend(pending.iter().map(|(type_name, field_name)| {
                ErrorFieldApplyBlocker::new(
                    file.path(),
                    type_name,
                    field_name,
                    ErrorFieldApplyBlockerReason::InvalidEdit,
                )
            }));
            return Ok(FilePlan {
                patched: None,
                converted: 0,
                blockers,
                units,
                edits: Vec::new(),
            });
        }
        patched.replace_range(edit.start..edit.end, &edit.replacement);
        limit = edit.start;
    }
    if dry_run {
        for edit in &edits {
            tracing::info!(
                path = %file.path().display(),
                from = %&file.source()[edit.start..edit.end],
                to = %edit.replacement,
                "dry run: error field edit"
            );
        }
    }
    Ok(FilePlan {
        patched: Some(patched),
        converted,
        blockers,
        units,
        edits,
    })
}

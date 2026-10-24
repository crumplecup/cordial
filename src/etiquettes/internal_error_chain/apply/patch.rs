//! Plan and validate edits for one shared parsed source file.

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{Item, Visibility};
use tracing::instrument;

use crate::config::ErrorFieldStrategy;
use crate::error::{CordialError, CordialResult};
use crate::loader::SharedParsedFile;

use super::super::type_graph::item_derives_error;
use super::initializers::{FieldKind, InitializerVisitor};
use super::lifetime;
use super::{
    Edit, ErrorConversionUnit, ErrorFieldApplyBlocker, ErrorFieldApplyBlockerReason,
    is_error_string_field, is_static_str, is_str_ref_with_lifetime, is_string, known_error_impls,
    known_static_strings, single_struct_lifetime, span_range,
};

#[instrument(level = "debug", skip(file, strategy, ir_units))]
pub(super) fn patch_file(
    file: &SharedParsedFile,
    strategy: ErrorFieldStrategy,
    ir_units: &[ErrorConversionUnit],
    dry_run: bool,
) -> CordialResult<(
    Option<String>,
    usize,
    Vec<ErrorFieldApplyBlocker>,
    Vec<ErrorConversionUnit>,
)> {
    let syntax = file.syntax();
    let error_impls = known_error_impls(syntax);
    let static_strings = known_static_strings(syntax);
    let mut edits = Vec::new();
    let mut converted = 0;
    let mut blockers = Vec::new();
    let mut pending = Vec::new();
    let mut units = Vec::new();
    for item in &syntax.items {
        let Item::Struct(item) = item else { continue };
        let name = item.ident.to_string();
        if !error_impls.contains(&name) && !item_derives_error(&item.attrs) {
            continue;
        }
        let syn::Fields::Named(fields) = &item.fields else {
            continue;
        };
        for field in &fields.named {
            let Some(field_name) = field.ident.as_ref().map(ToString::to_string) else {
                continue;
            };
            let kind = match field_name.as_str() {
                "file" => FieldKind::CallSiteFile,
                "source" | "kind" => continue,
                _ => FieldKind::StaticString,
            };
            let struct_lifetime = single_struct_lifetime(item);
            if !is_error_string_field(&field.ty) {
                continue;
            }
            let unit = ir_units
                .iter()
                .find(|unit| {
                    unit.declaration() == file.path()
                        && unit.type_name() == &name
                        && unit.field_name() == &field_name
                })
                .ok_or_else(|| {
                    CordialError::missing_conversion_unit(file.path(), &name, &field_name)
                })?
                .clone();
            let has_cross_file_reference =
                unit.sites().iter().any(|site| site.path() != file.path());
            let has_unknown_field_use = unit.blocks_existing_field_edit();
            let name_ambiguous = unit.name_ambiguous();
            let lifetime_candidate =
                lifetime::is_candidate(item, field, kind, struct_lifetime.as_deref(), strategy);
            if lifetime_candidate
                && !has_cross_file_reference
                && !name_ambiguous
                && matches!(item.vis, Visibility::Inherited)
                && matches!(field.vis, Visibility::Inherited)
            {
                if let Some(mut planned) = lifetime::plan(file, item, &field_name, &unit, strategy)
                {
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
            let (source_type, dynamic_lifetime) = match strategy {
                ErrorFieldStrategy::Borrowed if is_string(&field.ty) => {
                    let lifetime = struct_lifetime.as_deref();
                    (format!("&'{} str", lifetime.unwrap_or("static")), lifetime)
                }
                ErrorFieldStrategy::Owned if is_static_str(&field.ty) => {
                    ("String".to_string(), None)
                }
                ErrorFieldStrategy::Owned
                    if struct_lifetime
                        .as_deref()
                        .is_some_and(|lifetime| is_str_ref_with_lifetime(&field.ty, lifetime)) =>
                {
                    ("String".to_string(), struct_lifetime.as_deref())
                }
                _ => continue,
            };
            units.push(unit);
            if !matches!(item.vis, Visibility::Inherited)
                || !matches!(field.vis, Visibility::Inherited)
                || syntax.items.iter().filter(|candidate| matches!(candidate, Item::Struct(other) if other.ident == item.ident)).count() != 1
                || has_cross_file_reference
            {
                blockers.push(ErrorFieldApplyBlocker::new(file.path(), &name, &field_name, ErrorFieldApplyBlockerReason::OutsideFile));
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
            let mut visitor = InitializerVisitor::new(
                &name,
                &field_name,
                kind,
                strategy,
                file.source(),
                &static_strings,
                dynamic_lifetime,
                false,
            );
            visitor.visit_file(syntax);
            let Some(mut initializers) = visitor.finish() else {
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
            let Some(field_span) = span_range(file.source(), field.ty.span()) else {
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
    }
    apply_planned_edits(file, edits, converted, blockers, pending, units, dry_run)
}

#[instrument(level = "debug", skip(file, edits, blockers, pending, units))]
fn apply_planned_edits(
    file: &SharedParsedFile,
    mut edits: Vec<Edit>,
    converted: usize,
    mut blockers: Vec<ErrorFieldApplyBlocker>,
    pending: Vec<(String, String)>,
    units: Vec<ErrorConversionUnit>,
    dry_run: bool,
) -> CordialResult<(
    Option<String>,
    usize,
    Vec<ErrorFieldApplyBlocker>,
    Vec<ErrorConversionUnit>,
)> {
    if edits.is_empty() {
        return Ok((None, converted, blockers, units));
    }
    edits.sort_by_key(|edit| std::cmp::Reverse(edit.start));
    let mut patched = file.source().to_string();
    let mut limit = patched.len();
    for edit in &edits {
        if edit.end > limit || edit.start > edit.end {
            blockers.extend(pending.iter().map(|(type_name, field_name)| {
                ErrorFieldApplyBlocker::new(
                    file.path(),
                    type_name,
                    field_name,
                    ErrorFieldApplyBlockerReason::InvalidEdit,
                )
            }));
            return Ok((None, 0, blockers, units));
        }
        patched.replace_range(edit.start..edit.end, &edit.replacement);
        limit = edit.start;
    }
    if syn::parse_file(&patched).is_err() {
        blockers.extend(pending.iter().map(|(type_name, field_name)| {
            ErrorFieldApplyBlocker::new(
                file.path(),
                type_name,
                field_name,
                ErrorFieldApplyBlockerReason::InvalidEdit,
            )
        }));
        return Ok((None, 0, blockers, units));
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
    Ok((Some(patched), converted, blockers, units))
}

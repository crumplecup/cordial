//! Conservative string field storage edits for the selected error strategy.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{GenericParam, Item, ItemStruct, Type, Visibility};
use tracing::instrument;

use crate::config::{ErrorFieldStrategy, load_cordial_config};
use crate::error::CordialResult;
use crate::loader::{SharedParsedFile, SourceLoadView};
use crate::session::RunAll;
use crate::targets::discover_crate_targets;

use super::type_graph::{for_each_shared_src_rust_file, item_derives_error, trait_is_std_error};

mod initializers;
mod inventory;
mod lifetime;
mod names;
mod uses;

use self::initializers::{FieldKind, InitializerVisitor};
pub use self::inventory::{ErrorConversionSite, ErrorConversionSiteKind, ErrorConversionUnit};
use self::names::audit_names;
use self::uses::has_unhandled_field_use;

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
}

/// Compatibility name for the original file-only apply result.
pub use ErrorFieldApplySummary as ErrorFileApplySummary;

/// Convert simple string fields to the configured owned or borrowed form.
///
/// Only private error structs and private fields with recognized same-file
/// initializers are patched: `file` from `Location::caller().file()`, and other
/// string fields from literals, qualified same-module or block-scoped string
/// constants and statics, qualified built-in static string macros, explicit
/// static string constructor parameters, or typed static string local aliases.
/// Parameters tied to an existing private error lifetime can also be borrowed.
/// New lifetime propagation and cross-file constructors are left for manual
/// work.
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
    };
    if !policy.enabled() {
        return Ok(summary);
    }
    for target in discover_crate_targets(project_root, &RunAll)? {
        if only_crate.is_some_and(|name| name != target.crate_name()) {
            continue;
        }
        // One view keeps each source file parsed once while discovering the
        // reachable tree and inspecting its declarations and constructors.
        let source = SourceLoadView::for_standalone_scan();
        let mut files = Vec::new();
        for_each_shared_src_rust_file(&source, target.crate_root(), |path, _| {
            files.push(source.parsed_file(path, "error-file-apply")?);
            Ok(())
        })?;
        for file in &files {
            let (patched, converted, blockers, units) =
                patch_file(file, policy.field_strategy(), &files, dry_run);
            summary.conversion_units.extend(units);
            summary.converted_fields += converted;
            summary.unresolved += blockers.len();
            for blocker in &blockers {
                tracing::info!(path = %blocker.path.display(), type_name = %blocker.type_name, field = %blocker.field_name, reason = ?blocker.reason, "error field apply: candidate left unchanged");
            }
            summary.blockers.extend(blockers);
            if let Some(patched) = patched {
                if dry_run {
                    tracing::info!(path = %file.path().display(), converted, "dry run: would update error field storage");
                } else {
                    std::fs::write(file.path(), patched)?;
                }
                summary.changed_files += 1;
            }
        }
    }
    Ok(summary)
}

/// Compatibility name for the original file-only apply operation.
pub use run_error_field_apply as run_error_file_apply;

#[derive(Clone)]
struct Edit {
    start: usize,
    end: usize,
    replacement: String,
}

#[instrument(level = "debug", skip(file, strategy, files))]
fn patch_file(
    file: &SharedParsedFile,
    strategy: ErrorFieldStrategy,
    files: &[SharedParsedFile],
    dry_run: bool,
) -> (
    Option<String>,
    usize,
    Vec<ErrorFieldApplyBlocker>,
    Vec<ErrorConversionUnit>,
) {
    let syntax = file.syntax();
    let error_impls = known_error_impls(syntax);
    let static_strings = known_static_strings(syntax);
    let mut edits = Vec::new();
    let name_audit = audit_names(syntax);
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
            let unit = ErrorConversionUnit::collect(
                files,
                file.path(),
                &name,
                &field_name,
                name_audit.blocks(&name),
            );
            let has_cross_file_reference =
                unit.sites().iter().any(|site| site.path() != file.path());
            let has_unknown_field_use = unit.blocks_existing_field_edit();
            let lifetime_candidate =
                lifetime::is_candidate(item, field, kind, struct_lifetime.as_deref(), strategy);
            if lifetime_candidate
                && !has_cross_file_reference
                && !name_audit.blocks(&name)
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
            if name_audit.blocks(&name) {
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
            if has_unknown_field_use || has_unhandled_field_use(syntax, &name, &field_name) {
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
    if edits.is_empty() {
        return (None, converted, blockers, units);
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
            return (None, 0, blockers, units);
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
        return (None, 0, blockers, units);
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
    (Some(patched), converted, blockers, units)
}

#[instrument(level = "debug", skip(syntax))]
fn known_error_impls(syntax: &syn::File) -> BTreeSet<String> {
    syntax
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Impl(item)
                if item
                    .trait_
                    .as_ref()
                    .is_some_and(|(path, _)| trait_is_std_error(path)) =>
            {
                simple_type_name(&item.self_ty)
            }
            _ => None,
        })
        .collect()
}

#[instrument(level = "debug", skip(syntax))]
fn known_static_strings(syntax: &syn::File) -> BTreeSet<String> {
    let mut declarations: BTreeMap<String, Vec<bool>> = BTreeMap::new();
    for item in &syntax.items {
        let (name, ty, attrs, immutable) = match item {
            Item::Const(item) => (&item.ident, &item.ty, &item.attrs, true),
            Item::Static(item) => (
                &item.ident,
                &item.ty,
                &item.attrs,
                matches!(item.mutability, syn::StaticMutability::None),
            ),
            _ => continue,
        };
        declarations
            .entry(name.to_string())
            .or_default()
            .push(immutable && attrs.is_empty() && is_static_str(ty));
    }
    declarations
        .into_iter()
        .filter_map(|(name, declarations)| (declarations.as_slice() == [true]).then_some(name))
        .collect()
}

#[instrument(level = "debug", skip(ty))]
fn simple_type_name(ty: &Type) -> Option<String> {
    let Type::Path(path) = ty else { return None };
    if path.qself.is_some() || path.path.segments.len() != 1 {
        return None;
    }
    path.path
        .segments
        .first()
        .map(|segment| segment.ident.to_string())
}

#[instrument(level = "trace", skip(ty), ret)]
fn is_string(ty: &Type) -> bool {
    matches!(ty, Type::Path(path) if path.path.is_ident("String"))
}

#[instrument(level = "trace", skip(ty), ret)]
fn is_static_str(ty: &Type) -> bool {
    is_str_ref_with_lifetime(ty, "static")
}

#[instrument(level = "trace", skip(ty, lifetime), ret)]
fn is_str_ref_with_lifetime(ty: &Type, lifetime: &str) -> bool {
    matches!(ty, Type::Reference(reference)
        if reference.mutability.is_none()
            && reference.lifetime.as_ref().is_some_and(|named| named.ident == lifetime)
            && matches!(&*reference.elem, Type::Path(path) if path.path.is_ident("str")))
}

#[instrument(level = "trace", skip(item), ret)]
fn single_struct_lifetime(item: &ItemStruct) -> Option<String> {
    if item.generics.where_clause.is_some() || item.generics.params.len() != 1 {
        return None;
    }
    match item.generics.params.first()? {
        GenericParam::Lifetime(param) if param.attrs.is_empty() && param.bounds.is_empty() => {
            let lifetime = param.lifetime.ident.to_string();
            item.fields
                .iter()
                .any(|field| is_non_string_lifetime_anchor(&field.ty, &lifetime))
                .then_some(lifetime)
        }
        _ => None,
    }
}

#[instrument(level = "trace", skip(ty, lifetime), ret)]
fn is_non_string_lifetime_anchor(ty: &Type, lifetime: &str) -> bool {
    matches!(ty, Type::Reference(reference)
        if reference.lifetime.as_ref().is_some_and(|named| named.ident == lifetime)
            && !matches!(&*reference.elem, Type::Path(path) if path.path.is_ident("str")))
}

#[instrument(level = "debug", skip(source, span))]
fn span_range(source: &str, span: proc_macro2::Span) -> Option<(usize, usize)> {
    let offset = |line: usize, column: usize| {
        let start = source
            .split_inclusive('\n')
            .take(line.checked_sub(1)?)
            .map(str::len)
            .sum::<usize>();
        let offset = start.checked_add(column)?;
        source.is_char_boundary(offset).then_some(offset)
    };
    Some((
        offset(span.start().line, span.start().column)?,
        offset(span.end().line, span.end().column)?,
    ))
}

#[instrument(level = "debug", skip(source))]
fn source_has_ident(source: &str, name: &str) -> bool {
    source
        .split(|ch: char| !ch.is_alphanumeric() && ch != '_')
        .any(|part| part == name)
}

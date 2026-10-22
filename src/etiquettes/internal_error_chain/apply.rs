//! Conservative call-site file storage edits for the selected error strategy.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{Expr, ExprStruct, ImplItemFn, Item, ItemFn, ItemImpl, Member, Pat, Type, Visibility};
use tracing::instrument;

use crate::config::{ErrorFieldStrategy, load_cordial_config};
use crate::error::CordialResult;
use crate::loader::{SharedParsedFile, SourceLoadView};
use crate::session::RunAll;
use crate::targets::discover_crate_targets;

use super::type_graph::{for_each_shared_src_rust_file, item_derives_error, trait_is_std_error};

/// Result of changing call-site file fields and their recognized initializers.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters)]
pub struct ErrorFileApplySummary {
    /// Source files changed, or that would change under `--dry-run`.
    #[getter(copy)]
    changed_files: usize,
    /// Error types whose file field and every initializer were converted.
    #[getter(copy)]
    converted_fields: usize,
    /// Candidate error types left untouched because the conversion was incomplete.
    #[getter(copy)]
    unresolved: usize,
}

/// Convert simple call-site file fields to the configured owned or borrowed form.
///
/// Only a private error struct with a private `file` field and recognized
/// `Location::caller().file()` initializers in the same file is patched. Other
/// error fields, lifetime propagation, and cross-file constructors are left for
/// manual work.
#[instrument(level = "info", err(level = "warn"))]
pub fn run_error_file_apply(
    project_root: &Path,
    store_home: &Path,
    only_crate: Option<&str>,
    dry_run: bool,
) -> CordialResult<ErrorFileApplySummary> {
    let config = load_cordial_config(project_root, store_home);
    let policy = config.internal_error_chain();
    let mut summary = ErrorFileApplySummary {
        changed_files: 0,
        converted_fields: 0,
        unresolved: 0,
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
            let (patched, converted, unresolved) =
                patch_file(file, policy.field_strategy(), &files, dry_run);
            summary.converted_fields += converted;
            summary.unresolved += unresolved;
            if let Some(patched) = patched {
                if dry_run {
                    tracing::info!(path = %file.path().display(), converted, "dry run: would update error file storage");
                } else {
                    std::fs::write(file.path(), patched)?;
                }
                summary.changed_files += 1;
            }
        }
    }
    Ok(summary)
}

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
) -> (Option<String>, usize, usize) {
    let syntax = file.syntax();
    let error_impls: BTreeSet<String> = syntax
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
        .collect();
    let mut edits = Vec::new();
    let mut converted = 0;
    let mut unresolved = 0;
    for item in &syntax.items {
        let Item::Struct(item) = item else { continue };
        let name = item.ident.to_string();
        if !error_impls.contains(&name) && !item_derives_error(&item.attrs) {
            continue;
        }
        let syn::Fields::Named(fields) = &item.fields else {
            continue;
        };
        let Some(field) = fields
            .named
            .iter()
            .find(|field| field.ident.as_ref().is_some_and(|ident| ident == "file"))
        else {
            continue;
        };
        let source_type = match strategy {
            ErrorFieldStrategy::Borrowed if is_string(&field.ty) => "&'static str",
            ErrorFieldStrategy::Owned if is_static_str(&field.ty) => "String",
            _ => continue,
        };
        if !matches!(item.vis, Visibility::Inherited)
            || !matches!(field.vis, Visibility::Inherited)
            || syntax.items.iter().filter(|candidate| matches!(candidate, Item::Struct(other) if other.ident == item.ident)).count() != 1
            || files.iter().any(|other| {
                other.path() != file.path() && source_has_ident(other.source(), &name)
            })
        {
            unresolved += 1;
            tracing::info!(path = %file.path().display(), type_name = %name, "error file apply: public or cross-file type skipped");
            continue;
        }
        let mut visitor = InitializerVisitor::new(&name, strategy, file.source());
        visitor.visit_file(syntax);
        let Some(mut initializers) = visitor.finish() else {
            unresolved += 1;
            tracing::info!(path = %file.path().display(), type_name = %name, "error file apply: initializer not fully recognized");
            continue;
        };
        let Some(field_span) = span_range(file.source(), field.ty.span()) else {
            unresolved += 1;
            continue;
        };
        edits.push(Edit {
            start: field_span.0,
            end: field_span.1,
            replacement: source_type.to_string(),
        });
        edits.append(&mut initializers);
        converted += 1;
    }
    if edits.is_empty() {
        return (None, converted, unresolved);
    }
    edits.sort_by_key(|edit| std::cmp::Reverse(edit.start));
    let mut patched = file.source().to_string();
    let mut limit = patched.len();
    for edit in &edits {
        if edit.end > limit || edit.start > edit.end {
            return (None, 0, unresolved + converted);
        }
        patched.replace_range(edit.start..edit.end, &edit.replacement);
        limit = edit.start;
    }
    if syn::parse_file(&patched).is_err() {
        return (None, 0, unresolved + converted);
    }
    if dry_run {
        for edit in &edits {
            tracing::info!(
                path = %file.path().display(),
                from = %&file.source()[edit.start..edit.end],
                to = %edit.replacement,
                "dry run: error file edit"
            );
        }
    }
    (Some(patched), converted, unresolved)
}

struct InitializerVisitor<'a> {
    type_name: &'a str,
    strategy: ErrorFieldStrategy,
    source: &'a str,
    current_impl: Option<String>,
    caller_aliases: BTreeSet<String>,
    edits: Vec<Edit>,
    found: usize,
    unresolved: bool,
}

impl<'a> InitializerVisitor<'a> {
    #[instrument(level = "debug", skip(strategy, source))]
    fn new(type_name: &'a str, strategy: ErrorFieldStrategy, source: &'a str) -> Self {
        Self {
            type_name,
            strategy,
            source,
            current_impl: None,
            caller_aliases: BTreeSet::new(),
            edits: Vec::new(),
            found: 0,
            unresolved: false,
        }
    }

    #[instrument(level = "debug", skip(self))]
    fn finish(self) -> Option<Vec<Edit>> {
        (self.found > 0 && !self.unresolved).then_some(self.edits)
    }

    #[instrument(level = "debug", skip(self, block))]
    fn visit_block_with_aliases(&mut self, block: &syn::Block) {
        let previous = std::mem::replace(&mut self.caller_aliases, caller_aliases(block));
        syn::visit::visit_block(self, block);
        self.caller_aliases = previous;
    }
}

impl<'ast> Visit<'ast> for InitializerVisitor<'_> {
    #[instrument(level = "debug", skip(self, _item))]
    fn visit_item_mod(&mut self, _item: &'ast syn::ItemMod) {
        // A nested module has its own `Self` and privacy context.
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_item_impl(&mut self, item: &'ast ItemImpl) {
        let previous = std::mem::replace(&mut self.current_impl, simple_type_name(&item.self_ty));
        syn::visit::visit_item_impl(self, item);
        self.current_impl = previous;
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_item_fn(&mut self, item: &'ast ItemFn) {
        self.visit_block_with_aliases(&item.block);
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_impl_item_fn(&mut self, item: &'ast ImplItemFn) {
        self.visit_block_with_aliases(&item.block);
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_expr_struct(&mut self, item: &'ast ExprStruct) {
        let target = item.path.is_ident(self.type_name)
            || (item.path.is_ident("Self") && self.current_impl.as_deref() == Some(self.type_name));
        if !target
            && item
                .path
                .segments
                .last()
                .is_some_and(|segment| segment.ident == self.type_name)
        {
            self.unresolved = true;
        }
        if target {
            self.found += 1;
            if item.rest.is_some() {
                self.unresolved = true;
            } else if let Some(field) = item
                .fields
                .iter()
                .find(|field| matches!(&field.member, Member::Named(name) if name == "file"))
            {
                let replacement = file_initializer_replacement(
                    self.source,
                    &field.expr,
                    self.strategy,
                    &self.caller_aliases,
                );
                if let (Some((start, end)), Some(replacement)) =
                    (span_range(self.source, field.expr.span()), replacement)
                {
                    self.edits.push(Edit {
                        start,
                        end,
                        replacement,
                    });
                } else {
                    self.unresolved = true;
                }
            } else {
                self.unresolved = true;
            }
        }
        syn::visit::visit_expr_struct(self, item);
    }
}

#[instrument(level = "debug", skip(source, expr, strategy, aliases))]
fn file_initializer_replacement(
    source: &str,
    expr: &Expr,
    strategy: ErrorFieldStrategy,
    aliases: &BTreeSet<String>,
) -> Option<String> {
    let Expr::MethodCall(method) = expr else {
        return None;
    };
    let file_call = match strategy {
        ErrorFieldStrategy::Borrowed
            if method.method == "to_string" || method.method == "to_owned" =>
        {
            if !method.args.is_empty() {
                return None;
            }
            &*method.receiver
        }
        ErrorFieldStrategy::Owned => expr,
        _ => return None,
    };
    let Expr::MethodCall(file_method) = file_call else {
        return None;
    };
    if file_method.method != "file" || !file_method.args.is_empty() {
        return None;
    }
    let valid_receiver = match &*file_method.receiver {
        Expr::Path(path) => path
            .path
            .get_ident()
            .is_some_and(|name| aliases.contains(&name.to_string())),
        Expr::Call(call) => is_location_caller(call),
        _ => false,
    };
    if !valid_receiver {
        return None;
    }
    let (start, end) = span_range(source, file_call.span())?;
    let file_text = source.get(start..end)?;
    Some(match strategy {
        ErrorFieldStrategy::Borrowed => file_text.to_string(),
        ErrorFieldStrategy::Owned => format!("{file_text}.to_string()"),
    })
}

#[instrument(level = "debug", skip(block))]
fn caller_aliases(block: &syn::Block) -> BTreeSet<String> {
    let mut aliases: BTreeSet<String> = block.stmts.iter().filter_map(|stmt| {
        let syn::Stmt::Local(local) = stmt else { return None };
        let Pat::Ident(pat) = &local.pat else { return None };
        if pat.mutability.is_some() || !matches!(local.init.as_ref().map(|init| &*init.expr), Some(Expr::Call(call)) if is_location_caller(call)) {
            return None;
        }
        Some(pat.ident.to_string())
    }).collect();
    let mut bindings = BindingCounts::default();
    bindings.visit_block(block);
    aliases.retain(|name| bindings.0.get(name) == Some(&1));
    aliases
}

#[derive(Default)]
struct BindingCounts(BTreeMap<String, usize>);

impl<'ast> Visit<'ast> for BindingCounts {
    #[instrument(level = "debug", skip(self, pat))]
    fn visit_pat_ident(&mut self, pat: &'ast syn::PatIdent) {
        *self.0.entry(pat.ident.to_string()).or_default() += 1;
        syn::visit::visit_pat_ident(self, pat);
    }
}

#[instrument(level = "trace", skip(call))]
fn is_location_caller(call: &syn::ExprCall) -> bool {
    if !call.args.is_empty() {
        return false;
    }
    let Expr::Path(path) = &*call.func else {
        return false;
    };
    let names: Vec<String> = path
        .path
        .segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect();
    matches!(names.as_slice(), [location, caller] if location == "Location" && caller == "caller")
        || matches!(names.as_slice(), [root, panic, location, caller]
            if (root == "std" || root == "core") && panic == "panic" && location == "Location" && caller == "caller")
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
    matches!(ty, Type::Reference(reference)
        if reference.mutability.is_none()
            && reference.lifetime.as_ref().is_some_and(|lifetime| lifetime.ident == "static")
            && matches!(&*reference.elem, Type::Path(path) if path.path.is_ident("str")))
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

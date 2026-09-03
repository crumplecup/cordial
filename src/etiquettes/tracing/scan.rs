use std::collections::HashSet;
use std::path::Path;

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{Attribute, ImplItem, Item, ItemFn, ItemImpl, ItemMod, Type, Visibility};

use crate::error::CordialResult;
use crate::loader::module_path_from_src_file;

use super::classify::classify;
use super::display_types::{DisplayTypeFacts, collect_display_type_facts};
use super::recipe::recipe as instrument_recipe;
use super::types::{FunctionKind, FunctionRecord, VisibilityLabel};

use tracing::instrument;

/// Scan every `src/**/*.rs` file under `src_root`. `never_instrument`
/// names (already scoped to `crate_name`, qualified the same way
/// [`FunctionRecord::qualified_name`] is) never get recorded at all --
/// see [`super::call_graph::CallGraphFacts`] for how that set is computed.
#[instrument(level = "debug", skip(never_instrument), err(level = "warn"))]
pub fn scan_source_tree(
    src_root: &Path,
    project_root: &Path,
    crate_name: &str,
    extra_skip: &[String],
    never_instrument: &HashSet<String>,
) -> CordialResult<Vec<FunctionRecord>> {
    if !src_root.is_dir() {
        return Ok(Vec::new());
    }

    let mut records = Vec::new();
    for entry in walkdir::WalkDir::new(src_root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "rs") {
            continue;
        }
        let source = std::fs::read_to_string(path)?;
        let mut file_records = scan_rust_source(
            &source,
            path,
            src_root,
            project_root,
            crate_name,
            extra_skip,
            never_instrument,
        )?;
        records.append(&mut file_records);
    }

    records.sort_by(|a, b| {
        a.file()
            .cmp(b.file())
            .then(a.line().cmp(&b.line()))
            .then(a.qualified_name().cmp(b.qualified_name()))
    });
    Ok(records)
}

/// Parse one source file and return discovered functions (used by tests).
#[instrument(
    level = "debug",
    skip(source, file, never_instrument),
    err(level = "warn")
)]
pub fn scan_rust_source(
    source: &str,
    file: &Path,
    src_root: &Path,
    project_root: &Path,
    crate_name: &str,
    extra_skip: &[String],
    never_instrument: &HashSet<String>,
) -> CordialResult<Vec<FunctionRecord>> {
    let syntax = syn::parse_file(source)
        .map_err(|err| crate::error::CordialError::syn_parse(file.display().to_string(), err))?;
    let module_prefix = module_path_from_src_file(src_root, file);
    let rel_file = file
        .strip_prefix(project_root)
        .unwrap_or(file)
        .to_string_lossy()
        .replace('\\', "/");
    let display_types = collect_display_type_facts(&syntax.items);
    let mut visitor = FileScanVisitor {
        crate_name: crate_name.to_string(),
        rel_file,
        module_prefix,
        extra_skip,
        display_types,
        never_instrument,
        records: Vec::new(),
        error: None,
    };
    visitor.visit_file(&syntax);
    if let Some(error) = visitor.error {
        return Err(error);
    }
    Ok(visitor.records)
}

struct FileScanVisitor<'a> {
    crate_name: String,
    rel_file: String,
    module_prefix: Vec<String>,
    extra_skip: &'a [String],
    display_types: DisplayTypeFacts,
    never_instrument: &'a HashSet<String>,
    records: Vec<FunctionRecord>,
    error: Option<crate::error::CordialError>,
}

/// Every fact needed to record one function, bundled so
/// [`FileScanVisitor::record_fn`] takes one argument instead of seven.
struct RecordFnArgs<'a> {
    sig: &'a syn::Signature,
    attrs: &'a [Attribute],
    visibility: &'a Visibility,
    span: proc_macro2::Span,
    kind: FunctionKind,
    local_name: &'a str,
    body: Option<&'a syn::Block>,
}

impl FileScanVisitor<'_> {
    #[instrument(level = "debug", skip(self))]
    fn qualify(&self, local: &str) -> String {
        if self.module_prefix.is_empty() {
            local.to_string()
        } else {
            format!("{}::{local}", self.module_prefix.join("::"))
        }
    }

    #[instrument(level = "debug", skip(self, args))]
    fn record_fn(&mut self, args: RecordFnArgs<'_>) {
        if args.sig.constness.is_some() {
            // `tracing::instrument` categorically rejects `const fn`
            // (`error: the #[instrument] attribute may not be used with
            // const fns`) -- never record one as needing instrumentation,
            // so neither the checklist nor `--apply` ever proposes it.
            return;
        }
        let qualified_name = self.qualify(args.local_name);
        let instrumented = args.attrs.iter().any(crate::enricher::is_instrument_attr);
        let proof_only = self.never_instrument.contains(&qualified_name);
        if proof_only && !instrumented {
            // Proof-only and no span: attenuation has nothing to remove,
            // and missing-instrument must not push toward adding one.
            return;
        }
        let prover_visible_instrument = args.attrs.iter().any(|attr| {
            crate::enricher::is_instrument_attr(attr)
                && !crate::enricher::is_gated_instrument_attr(attr)
        });
        let line = args.span.start().line as u32;
        let ctx = match classify(
            &args.sig.ident.to_string(),
            args.sig,
            args.kind,
            args.body,
            &self.display_types,
        ) {
            Ok(ctx) => ctx,
            Err(error) => {
                if self.error.is_none() {
                    self.error = Some(error);
                }
                return;
            }
        };
        let recipe = match instrument_recipe(&ctx, self.extra_skip) {
            Ok(recipe) => recipe,
            Err(error) => {
                if self.error.is_none() {
                    self.error = Some(error);
                }
                return;
            }
        };
        if self.error.is_some() {
            return;
        }
        match FunctionRecord::builder()
            .crate_name(self.crate_name.clone())
            .qualified_name(qualified_name)
            .kind(args.kind)
            .visibility(visibility_label(args.visibility))
            .file(self.rel_file.clone())
            .line(line)
            .instrumented(instrumented)
            .proof_only(proof_only)
            .prover_visible_instrument(prover_visible_instrument)
            .has_error_path_event(ctx.has_error_path_event())
            .param_names(ctx.param_names().clone())
            .role(ctx.role())
            .complexity(ctx.complexity())
            .recipe(recipe)
            .build()
        {
            Ok(record) => self.records.push(record),
            Err(error) => self.error = Some(error),
        }
    }

    #[instrument(level = "debug", skip(self, items))]
    fn visit_module_items(&mut self, items: &[Item], module_prefix: &[String]) {
        let prev = self.module_prefix.clone();
        self.module_prefix = module_prefix.to_vec();
        for item in items {
            self.visit_item(item);
        }
        self.module_prefix = prev;
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_item(&mut self, item: &Item) {
        match item {
            Item::Fn(item_fn) => {
                self.record_fn(RecordFnArgs {
                    sig: &item_fn.sig,
                    attrs: &item_fn.attrs,
                    visibility: &item_fn.vis,
                    span: item_fn.span(),
                    kind: FunctionKind::Free,
                    local_name: &item_fn.sig.ident.to_string(),
                    body: Some(&item_fn.block),
                });
            }
            Item::Mod(item_mod) => self.visit_mod(item_mod),
            Item::Impl(item_impl) => self.visit_impl(item_impl),
            _ => {}
        }
    }

    #[instrument(level = "debug", skip(self, item_mod))]
    fn visit_mod(&mut self, item_mod: &ItemMod) {
        if crate::enricher::is_cfg_test(&item_mod.attrs) {
            return;
        }
        let Some((_, items)) = &item_mod.content else {
            return;
        };
        let mut nested = self.module_prefix.clone();
        nested.push(item_mod.ident.to_string());
        self.visit_module_items(items, &nested);
    }

    #[instrument(level = "debug", skip(self, item_impl))]
    fn visit_impl(&mut self, item_impl: &ItemImpl) {
        let self_ty = self_type_key(&item_impl.self_ty);
        let trait_name = item_impl
            .trait_
            .as_ref()
            .map(|(_, path, _)| syn_path_label(path));
        for impl_item in &item_impl.items {
            let ImplItem::Fn(method) = impl_item else {
                continue;
            };
            let local = impl_method_local_name(&self_ty, trait_name.as_deref(), &method.sig.ident);
            let kind = if trait_name.is_some() {
                FunctionKind::TraitImplMethod
            } else {
                FunctionKind::InherentMethod
            };
            self.record_fn(RecordFnArgs {
                sig: &method.sig,
                attrs: &method.attrs,
                visibility: &method.vis,
                span: method.span(),
                kind,
                local_name: &local,
                body: Some(&method.block),
            });
        }
    }
}

impl<'ast> Visit<'ast> for FileScanVisitor<'_> {
    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        self.record_fn(RecordFnArgs {
            sig: &node.sig,
            attrs: &node.attrs,
            visibility: &node.vis,
            span: node.span(),
            kind: FunctionKind::Free,
            local_name: &node.sig.ident.to_string(),
            body: Some(&node.block),
        });
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_mod(&mut self, node: &'ast ItemMod) {
        self.visit_mod(node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_impl(&mut self, node: &'ast ItemImpl) {
        self.visit_impl(node);
    }
}

#[instrument(level = "debug", skip(vis))]
fn visibility_label(vis: &Visibility) -> VisibilityLabel {
    match vis {
        Visibility::Public(_) => VisibilityLabel::Public,
        Visibility::Restricted(restricted) => {
            if restricted.path.is_ident("crate") {
                VisibilityLabel::PubCrate
            } else if restricted.path.is_ident("super") {
                VisibilityLabel::PubSuper
            } else {
                VisibilityLabel::PubInPath(restricted.path.segments[0].ident.to_string())
            }
        }
        Visibility::Inherited => VisibilityLabel::Private,
    }
}

#[instrument(level = "debug", skip(ty))]
pub(super) fn type_label(ty: &Type) -> String {
    match ty {
        Type::Path(type_path) => syn_path_label(&type_path.path),
        Type::Reference(reference) => type_label(&reference.elem),
        Type::Paren(paren) => type_label(&paren.elem),
        Type::Group(group) => type_label(&group.elem),
        _ => "?".to_string(),
    }
}

#[instrument(level = "debug", skip(path))]
pub(super) fn syn_path_label(path: &syn::Path) -> String {
    path.segments
        .last()
        .map(|segment| segment.ident.to_string())
        .unwrap_or_else(|| "?".to_string())
}

/// A `::`-free rendering of a type that *keeps* its generic arguments,
/// so `RustStdStandard<AtomicI8>` and `RustStdStandard<AtomicI16>`
/// produce distinct keys -- plain [`type_label`] collapses both to bare
/// `RustStdStandard`, which is exactly the collapse that let one
/// `impl Trait for RustStdStandard<T>` method stand in for every sibling
/// impl in the same module (checklist coverage hole). Lifetimes and path
/// prefixes are dropped; tuples, refs, slices, arrays and pointers are
/// rendered structurally; anything else falls back to [`type_label`].
#[instrument(level = "debug", skip(ty))]
pub(super) fn self_type_key(ty: &Type) -> String {
    match ty {
        Type::Path(type_path) => {
            let Some(segment) = type_path.path.segments.last() else {
                return "?".to_string();
            };
            let base = segment.ident.to_string();
            let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
                return base;
            };
            let rendered: Vec<String> = args
                .args
                .iter()
                .filter_map(|arg| match arg {
                    syn::GenericArgument::Type(inner) => Some(self_type_key(inner)),
                    syn::GenericArgument::Const(expr) => Some(const_arg_label(expr)),
                    _ => None,
                })
                .collect();
            if rendered.is_empty() {
                base
            } else {
                format!("{base}<{}>", rendered.join(", "))
            }
        }
        Type::Reference(reference) => {
            let inner = self_type_key(&reference.elem);
            if reference.mutability.is_some() {
                format!("&mut {inner}")
            } else {
                format!("&{inner}")
            }
        }
        Type::Ptr(ptr) => {
            let inner = self_type_key(&ptr.elem);
            if ptr.mutability.is_some() {
                format!("*mut {inner}")
            } else {
                format!("*const {inner}")
            }
        }
        Type::Tuple(tuple) => {
            let parts: Vec<String> = tuple.elems.iter().map(self_type_key).collect();
            format!("({})", parts.join(", "))
        }
        Type::Slice(slice) => format!("[{}]", self_type_key(&slice.elem)),
        Type::Array(array) => {
            format!("[{}; {}]", self_type_key(&array.elem), const_arg_label(&array.len))
        }
        Type::Paren(paren) => self_type_key(&paren.elem),
        Type::Group(group) => self_type_key(&group.elem),
        _ => type_label(ty),
    }
}

/// A short label for a const generic argument (`IntoIter<i32, 3>`'s `3`,
/// `[u8; 4]`'s `4`). Integer literals render as their digits; anything
/// more elaborate collapses to `_` -- distinctness only needs the common
/// literal case.
#[instrument(level = "trace", skip(expr))]
fn const_arg_label(expr: &syn::Expr) -> String {
    match expr {
        syn::Expr::Lit(syn::ExprLit { lit: syn::Lit::Int(int), .. }) => {
            int.base10_digits().to_string()
        }
        syn::Expr::Lit(syn::ExprLit { lit: syn::Lit::Bool(b), .. }) => b.value.to_string(),
        syn::Expr::Path(path) => syn_path_label(&path.path),
        _ => "_".to_string(),
    }
}

/// The qualified local name (module prefix not included) an impl
/// method is recorded under -- UFCS-qualified for a trait impl
/// (`<{self_ty} as {trait_name}>::{method}`, e.g.
/// `<RustStdStandard<AtomicI8> as KaniWitness>::proof`, so each type's
/// impl of a shared trait is a distinct checklist row), self-type
/// qualified otherwise (`{self_ty}::{method}`). Shared with
/// [`super::call_graph::CallGraphFacts`] so a call written as
/// `Type::method(..)` (the real syntactic form a trait impl method is
/// actually *called* with -- UFCS through the type, not the trait) can
/// still be resolved back to the same key this function is recorded
/// under, and with [`crate::enricher::AttributeEnricher`] so an existing
/// `#[instrument]` attaches to the right node.
#[instrument(level = "trace", skip(method_ident))]
pub(super) fn impl_method_local_name(
    self_ty: &str,
    trait_name: Option<&str>,
    method_ident: &syn::Ident,
) -> String {
    match trait_name {
        Some(trait_name) => format!("<{self_ty} as {trait_name}>::{method_ident}"),
        None => format!("{self_ty}::{method_ident}"),
    }
}

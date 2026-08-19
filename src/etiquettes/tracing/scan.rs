use std::path::Path;

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{Attribute, ImplItem, Item, ItemFn, ItemImpl, ItemMod, Type, Visibility};

use crate::error::CordialResult;
use crate::loader::module_path_from_src_file;

use super::classify::classify;
use super::recipe::recipe as instrument_recipe;
use super::types::{FunctionKind, FunctionRecord, VisibilityLabel};

use tracing::instrument;
/// Scan every `src/**/*.rs` file under `src_root`.
#[instrument(level = "debug", err(level = "warn"))]
pub fn scan_source_tree(
    src_root: &Path,
    project_root: &Path,
    crate_name: &str,
    extra_skip: &[String],
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
        )?;
        records.append(&mut file_records);
    }

    records.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then(a.line.cmp(&b.line))
            .then(a.qualified_name.cmp(&b.qualified_name))
    });
    Ok(records)
}

/// Parse one source file and return discovered functions (used by tests).
#[instrument(level = "debug", skip(source, file), err(level = "warn"))]
pub fn scan_rust_source(
    source: &str,
    file: &Path,
    src_root: &Path,
    project_root: &Path,
    crate_name: &str,
    extra_skip: &[String],
) -> CordialResult<Vec<FunctionRecord>> {
    let syntax = syn::parse_file(source)
        .map_err(|err| crate::error::CordialError::syn_parse(file.display().to_string(), err))?;
    let module_prefix = module_path_from_src_file(src_root, file);
    let rel_file = file
        .strip_prefix(project_root)
        .unwrap_or(file)
        .to_string_lossy()
        .replace('\\', "/");
    let mut visitor = FileScanVisitor {
        crate_name: crate_name.to_string(),
        rel_file,
        module_prefix,
        extra_skip,
        records: Vec::new(),
    };
    visitor.visit_file(&syntax);
    Ok(visitor.records)
}

struct FileScanVisitor<'a> {
    crate_name: String,
    rel_file: String,
    module_prefix: Vec<String>,
    extra_skip: &'a [String],
    records: Vec<FunctionRecord>,
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

    #[instrument(level = "debug", skip(self, sig, attrs, visibility, span, kind, body))]
    fn record_fn(
        &mut self,
        sig: &syn::Signature,
        attrs: &[Attribute],
        visibility: &Visibility,
        span: proc_macro2::Span,
        kind: FunctionKind,
        local_name: &str,
        body: Option<&syn::Block>,
    ) {
        let line = span.start().line as u32;
        let ctx = classify(&sig.ident.to_string(), sig, kind, body);
        let recipe = instrument_recipe(&ctx, self.extra_skip);
        self.records.push(FunctionRecord {
            crate_name: self.crate_name.clone(),
            qualified_name: self.qualify(local_name),
            kind,
            visibility: visibility_label(visibility),
            file: self.rel_file.clone(),
            line,
            instrumented: is_instrumented(attrs),
            has_error_path_event: ctx.has_error_path_event,
            param_names: ctx.param_names.clone(),
            role: ctx.role,
            complexity: ctx.complexity,
            recipe,
        });
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
                self.record_fn(
                    &item_fn.sig,
                    &item_fn.attrs,
                    &item_fn.vis,
                    item_fn.span(),
                    FunctionKind::Free,
                    &item_fn.sig.ident.to_string(),
                    Some(&item_fn.block),
                );
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
        let self_ty = type_label(&item_impl.self_ty);
        let trait_name = item_impl
            .trait_
            .as_ref()
            .map(|(_, path, _)| syn_path_label(path));
        for impl_item in &item_impl.items {
            let ImplItem::Fn(method) = impl_item else {
                continue;
            };
            let local = if let Some(trait_name) = trait_name.clone() {
                format!("{trait_name}::{}", method.sig.ident)
            } else {
                format!("{self_ty}::{}", method.sig.ident)
            };
            let kind = if trait_name.is_some() {
                FunctionKind::TraitImplMethod
            } else {
                FunctionKind::InherentMethod
            };
            self.record_fn(
                &method.sig,
                &method.attrs,
                &method.vis,
                method.span(),
                kind,
                &local,
                Some(&method.block),
            );
        }
    }
}

impl<'ast> Visit<'ast> for FileScanVisitor<'_> {
    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        self.record_fn(
            &node.sig,
            &node.attrs,
            &node.vis,
            node.span(),
            FunctionKind::Free,
            &node.sig.ident.to_string(),
            Some(&node.block),
        );
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

#[instrument(level = "trace", skip(attrs), ret)]
fn is_instrumented(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        let path = attr.path();
        path.is_ident("instrument")
            || (path.segments.len() == 2
                && path.segments[0].ident == "tracing"
                && path.segments[1].ident == "instrument")
    })
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
fn type_label(ty: &Type) -> String {
    match ty {
        Type::Path(type_path) => syn_path_label(&type_path.path),
        Type::Reference(reference) => type_label(&reference.elem),
        Type::Paren(paren) => type_label(&paren.elem),
        Type::Group(group) => type_label(&group.elem),
        _ => "?".to_string(),
    }
}

#[instrument(level = "debug", skip(path))]
fn syn_path_label(path: &syn::Path) -> String {
    path.segments
        .last()
        .map(|segment| segment.ident.to_string())
        .unwrap_or_else(|| "?".to_string())
}

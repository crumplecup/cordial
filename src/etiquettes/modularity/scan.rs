//! syn-based scan for oversized files, function/method bodies, types-per-file,
//! and per-module size analytics.

use std::path::{Path, PathBuf};

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{ItemFn, ItemImpl, ItemMod, ItemTrait};

use crate::error::CordialResult;
use crate::loader::module_path_from_src_file;

use super::types::{ModularityKind, ModularitySiteRecord, ModularityThresholds};

use tracing::instrument;
#[instrument(level = "debug", skip(thresholds), err(level = "warn"))]
pub fn scan_source_tree(
    src_root: &Path,
    crate_root: &Path,
    thresholds: ModularityThresholds,
) -> CordialResult<Vec<ModularitySiteRecord>> {
    let mut findings = Vec::new();
    if !src_root.is_dir() {
        return Ok(findings);
    }

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
        findings.extend(scan_rust_source(
            &source, path, src_root, crate_root, thresholds,
        )?);
    }

    findings.sort_by(|left, right| {
        right
            .lines
            .cmp(&left.lines)
            .then_with(|| left.kind.as_str().cmp(right.kind.as_str()))
            .then_with(|| left.file.cmp(&right.file))
            .then_with(|| left.context.cmp(&right.context))
    });

    Ok(findings)
}

#[instrument(level = "debug", skip(source, file, thresholds), err(level = "warn"))]
pub fn scan_rust_source(
    source: &str,
    file: &Path,
    src_root: &Path,
    crate_root: &Path,
    thresholds: ModularityThresholds,
) -> CordialResult<Vec<ModularitySiteRecord>> {
    let mut findings = Vec::new();
    maybe_push_file_finding(source, file, crate_root, thresholds, &mut findings)?;

    let syntax = syn::parse_file(source)
        .map_err(|err| crate::error::CordialError::syn_parse(file.display().to_string(), err))?;
    maybe_push_types_finding(&syntax, file, crate_root, thresholds, &mut findings);
    let module_prefix = module_path_from_src_file(src_root, file);
    push_module_size_records(
        source,
        &syntax,
        file,
        crate_root,
        &module_prefix,
        &mut findings,
    );
    let mut visitor = ModularityScanVisitor {
        file: file.to_path_buf(),
        crate_root: crate_root.to_path_buf(),
        module_prefix,
        impl_type: None,
        fn_stack: Vec::new(),
        file_lines: count_source_lines(source),
        thresholds,
        findings: Vec::new(),
    };
    visitor.visit_file(&syntax);
    findings.append(&mut visitor.findings);
    Ok(findings)
}

#[instrument(
    level = "debug",
    skip(source, file, thresholds, findings),
    err(level = "warn")
)]
fn maybe_push_file_finding(
    source: &str,
    file: &Path,
    crate_root: &Path,
    thresholds: ModularityThresholds,
    findings: &mut Vec<ModularitySiteRecord>,
) -> CordialResult<()> {
    let lines = count_source_lines(source);
    if lines < thresholds.file_inventory_min_lines {
        return Ok(());
    }
    findings.push(ModularitySiteRecord {
        kind: ModularityKind::File,
        context: String::new(),
        file: relative_source_path(file, crate_root),
        line: 1,
        lines,
        inline: false,
    });
    Ok(())
}

#[instrument(level = "debug", skip(source, syntax, file, findings))]
fn push_module_size_records(
    source: &str,
    syntax: &syn::File,
    file: &Path,
    crate_root: &Path,
    module_prefix: &[String],
    findings: &mut Vec<ModularitySiteRecord>,
) {
    let rel_file = relative_source_path(file, crate_root);
    findings.push(ModularitySiteRecord {
        kind: ModularityKind::ModuleSize,
        context: module_path_label(module_prefix),
        file: rel_file.clone(),
        line: 1,
        lines: count_source_lines(source),
        inline: false,
    });
    collect_inline_module_sizes(&syntax.items, module_prefix, rel_file, findings);
}

#[instrument(level = "debug", skip(items, file, findings))]
fn collect_inline_module_sizes(
    items: &[syn::Item],
    module_prefix: &[String],
    file: PathBuf,
    findings: &mut Vec<ModularitySiteRecord>,
) {
    for item in items {
        let syn::Item::Mod(item_mod) = item else {
            continue;
        };
        if is_cfg_test(&item_mod.attrs) {
            continue;
        }
        let Some((_, nested)) = &item_mod.content else {
            continue;
        };
        let mut nested_prefix = module_prefix.to_vec();
        nested_prefix.push(item_mod.ident.to_string());
        findings.push(ModularitySiteRecord {
            kind: ModularityKind::ModuleSize,
            context: module_path_label(&nested_prefix),
            file: file.clone(),
            line: item_mod.span().start().line as u32,
            lines: span_line_count(item_mod.span()),
            inline: true,
        });
        collect_inline_module_sizes(nested, &nested_prefix, file.clone(), findings);
    }
}

#[instrument(level = "debug")]
fn module_path_label(parts: &[String]) -> String {
    if parts.is_empty() {
        "<crate>".to_string()
    } else {
        parts.join("::")
    }
}

#[instrument(level = "debug", skip(syntax, file, thresholds, findings))]
fn maybe_push_types_finding(
    syntax: &syn::File,
    file: &Path,
    crate_root: &Path,
    thresholds: ModularityThresholds,
    findings: &mut Vec<ModularitySiteRecord>,
) {
    let names = file_type_names(&syntax.items);
    let types = u32::try_from(names.len()).unwrap_or(u32::MAX);
    if types <= thresholds.max_types_per_file {
        return;
    }
    findings.push(ModularitySiteRecord {
        kind: ModularityKind::TypesPerFile,
        context: names.join(", "),
        file: relative_source_path(file, crate_root),
        line: 1,
        lines: types,
        inline: false,
    });
}

#[instrument(level = "debug", skip(items))]
fn file_type_names(items: &[syn::Item]) -> Vec<String> {
    let mut names = Vec::new();
    collect_type_names(items, &mut names);
    names
}

#[instrument(level = "debug", skip(items))]
fn collect_type_names(items: &[syn::Item], names: &mut Vec<String>) {
    for item in items {
        match item {
            syn::Item::Struct(item) if !is_cfg_test(&item.attrs) => {
                names.push(item.ident.to_string());
            }
            syn::Item::Enum(item) if !is_cfg_test(&item.attrs) => {
                names.push(item.ident.to_string());
            }
            syn::Item::Union(item) if !is_cfg_test(&item.attrs) => {
                names.push(item.ident.to_string());
            }
            syn::Item::Trait(item) if !is_cfg_test(&item.attrs) => {
                names.push(item.ident.to_string());
            }
            syn::Item::Mod(item_mod) if !is_cfg_test(&item_mod.attrs) => {
                if let Some((_, nested)) = &item_mod.content {
                    collect_type_names(nested, names);
                }
            }
            _ => {}
        }
    }
}

#[instrument(level = "debug", skip(file))]
fn relative_source_path(file: &Path, crate_root: &Path) -> PathBuf {
    file.strip_prefix(crate_root)
        .map(Path::to_path_buf)
        .unwrap_or_else(|_| file.to_path_buf())
}

#[instrument(level = "debug", skip(source))]
fn count_source_lines(source: &str) -> u32 {
    u32::try_from(source.lines().count()).unwrap_or(u32::MAX)
}

struct ModularityScanVisitor {
    file: PathBuf,
    crate_root: PathBuf,
    module_prefix: Vec<String>,
    impl_type: Option<String>,
    fn_stack: Vec<String>,
    file_lines: u32,
    thresholds: ModularityThresholds,
    findings: Vec<ModularitySiteRecord>,
}

impl ModularityScanVisitor {
    #[instrument(level = "debug", skip(self))]
    fn site_context(&self) -> String {
        let mut parts = self.module_prefix.clone();
        if let Some(ty) = &self.impl_type {
            parts.push(ty.clone());
        }
        parts.extend(self.fn_stack.iter().cloned());
        if parts.is_empty() {
            "<crate>".to_string()
        } else {
            parts.join("::")
        }
    }

    #[instrument(level = "debug", skip(self, span))]
    fn check_function(&mut self, span: proc_macro2::Span) {
        let lines = span_line_count(span);
        if lines < self.thresholds.function_scan_min_lines(self.file_lines) {
            return;
        }
        self.findings.push(ModularitySiteRecord {
            kind: ModularityKind::Function,
            context: self.site_context(),
            file: relative_source_path(&self.file, &self.crate_root),
            line: span.start().line as u32,
            lines,
            inline: false,
        });
    }

    #[instrument(level = "debug", skip(self, attrs, block))]
    fn check_function_body(&mut self, attrs: &[syn::Attribute], block: &syn::Block) {
        if is_cfg_test(attrs) {
            return;
        }
        self.check_function(block.span());
    }

    #[instrument(level = "debug", skip(self, items))]
    fn visit_module_items(&mut self, items: &[syn::Item], module_prefix: &[String]) {
        let prev_prefix = self.module_prefix.clone();
        self.module_prefix = module_prefix.to_vec();
        for item in items {
            syn::visit::visit_item(self, item);
        }
        self.module_prefix = prev_prefix;
    }

    #[instrument(level = "debug", skip(self, item_mod))]
    fn visit_mod(&mut self, item_mod: &ItemMod) {
        if is_cfg_test(&item_mod.attrs) {
            return;
        }
        let Some((_, items)) = &item_mod.content else {
            return;
        };
        let mut nested = self.module_prefix.clone();
        nested.push(item_mod.ident.to_string());
        self.visit_module_items(items, &nested);
    }
}

impl<'ast> Visit<'ast> for ModularityScanVisitor {
    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_mod(&mut self, node: &'ast ItemMod) {
        self.visit_mod(node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        if is_cfg_test(&node.attrs) {
            return;
        }
        self.fn_stack.push(node.sig.ident.to_string());
        self.check_function_body(&node.attrs, &node.block);
        syn::visit::visit_item_fn(self, node);
        self.fn_stack.pop();
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_impl(&mut self, node: &'ast ItemImpl) {
        if is_cfg_test(&node.attrs) {
            return;
        }
        let prev = self.impl_type.clone();
        self.impl_type = Some(type_label(&node.self_ty));
        syn::visit::visit_item_impl(self, node);
        self.impl_type = prev;
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        if is_cfg_test(&node.attrs) {
            return;
        }
        self.fn_stack.push(node.sig.ident.to_string());
        self.check_function_body(&node.attrs, &node.block);
        syn::visit::visit_impl_item_fn(self, node);
        self.fn_stack.pop();
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_trait(&mut self, node: &'ast ItemTrait) {
        if is_cfg_test(&node.attrs) {
            return;
        }
        let prev = self.impl_type.clone();
        self.impl_type = Some(node.ident.to_string());
        syn::visit::visit_item_trait(self, node);
        self.impl_type = prev;
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_trait_item_fn(&mut self, node: &'ast syn::TraitItemFn) {
        if is_cfg_test(&node.attrs) {
            return;
        }
        let Some(block) = &node.default else {
            return;
        };
        self.fn_stack.push(node.sig.ident.to_string());
        self.check_function_body(&node.attrs, block);
        syn::visit::visit_trait_item_fn(self, node);
        self.fn_stack.pop();
    }

    #[instrument(level = "debug", skip(self, _node))]
    fn visit_expr_closure(&mut self, _node: &'ast syn::ExprClosure) {}
}

#[instrument(level = "debug", skip(span))]
fn span_line_count(span: proc_macro2::Span) -> u32 {
    let start = span.start().line;
    let end = span.end().line;
    u32::try_from(end.saturating_sub(start).saturating_add(1)).unwrap_or(u32::MAX)
}

#[instrument(level = "trace", skip(attrs))]
fn is_cfg_test(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        let syn::Meta::List(list) = &attr.meta else {
            return false;
        };
        if !list.path.is_ident("cfg") {
            return false;
        }
        list.tokens.to_string().replace(' ', "") == "test"
    })
}

#[instrument(level = "debug", skip(ty))]
fn type_label(ty: &syn::Type) -> String {
    match ty {
        syn::Type::Path(type_path) => path_label(&type_path.path),
        syn::Type::Reference(reference) => type_label(&reference.elem),
        syn::Type::Paren(paren) => type_label(&paren.elem),
        syn::Type::Group(group) => type_label(&group.elem),
        _ => "?".to_string(),
    }
}

#[instrument(level = "debug", skip(path))]
fn path_label(path: &syn::Path) -> String {
    path.segments
        .last()
        .map(|segment| segment.ident.to_string())
        .unwrap_or_else(|| "?".to_string())
}

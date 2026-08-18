//! syn-based scan for `#[allow(...)]` attributes.

use std::path::{Path, PathBuf};

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{
    Attribute, Expr, Field, File, ImplItemFn, ItemFn, ItemImpl, ItemMod, Meta, Type, Variant,
};

use crate::error::CordialResult;
use crate::loader::{module_path_from_src_file, path_has_fixtures, quality_scan_trees};

use super::types::{AllowRuleId, AllowSiteRecord};

use tracing::instrument;
#[instrument(level = "debug", err(level = "warn"))]
pub fn scan_crate_allows(crate_root: &Path) -> CordialResult<Vec<AllowSiteRecord>> {
    let mut findings = Vec::new();
    for tree_root in quality_scan_trees(crate_root) {
        findings.extend(scan_source_tree(&tree_root, crate_root)?);
    }

    findings.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then(a.line.cmp(&b.line))
            .then(a.context.cmp(&b.context))
            .then(a.snippet.cmp(&b.snippet))
    });

    Ok(findings)
}

#[instrument(level = "debug", err(level = "warn"))]
pub fn scan_source_tree(
    tree_root: &Path,
    crate_root: &Path,
) -> CordialResult<Vec<AllowSiteRecord>> {
    let mut findings = Vec::new();
    if !tree_root.is_dir() {
        return Ok(findings);
    }

    for entry in walkdir::WalkDir::new(tree_root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "rs") {
            continue;
        }
        if path_has_fixtures(path, crate_root) {
            continue;
        }
        let source = std::fs::read_to_string(path)?;
        findings.extend(scan_rust_source(&source, path, tree_root, crate_root)?);
    }

    Ok(findings)
}

#[instrument(level = "debug", skip(source, file), err(level = "warn"))]
pub fn scan_rust_source(
    source: &str,
    file: &Path,
    tree_root: &Path,
    crate_root: &Path,
) -> CordialResult<Vec<AllowSiteRecord>> {
    let syntax = syn::parse_file(source)
        .map_err(|err| crate::error::CordialError::syn_parse(file.display().to_string(), err))?;
    let module_prefix = module_path_from_src_file(tree_root, file);
    let mut visitor = AllowScanVisitor {
        file: file.to_path_buf(),
        crate_root: crate_root.to_path_buf(),
        module_prefix,
        impl_type: None,
        fn_stack: Vec::new(),
        findings: Vec::new(),
    };
    visitor.visit_file(&syntax);
    Ok(visitor.findings)
}

struct AllowScanVisitor {
    file: PathBuf,
    crate_root: PathBuf,
    module_prefix: Vec<String>,
    impl_type: Option<String>,
    fn_stack: Vec<String>,
    findings: Vec<AllowSiteRecord>,
}

impl AllowScanVisitor {
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

    fn check_attrs(&mut self, attrs: &[Attribute]) {
        for attr in attrs {
            let Some(snippet) = allow_snippet(attr) else {
                continue;
            };
            let mut file = self.file.clone();
            if let Ok(rel) = file.strip_prefix(&self.crate_root) {
                file = rel.to_path_buf();
            }
            self.findings.push(AllowSiteRecord {
                rule_id: AllowRuleId::Attr001,
                context: self.site_context(),
                file,
                line: attr.span().start().line as u32,
                snippet,
            });
        }
    }

    fn visit_module_items(&mut self, items: &[syn::Item], module_prefix: &[String]) {
        let prev_prefix = self.module_prefix.clone();
        self.module_prefix = module_prefix.to_vec();
        for item in items {
            syn::visit::visit_item(self, item);
        }
        self.module_prefix = prev_prefix;
    }

    fn visit_mod(&mut self, item_mod: &ItemMod) {
        self.check_attrs(&item_mod.attrs);
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

impl<'ast> Visit<'ast> for AllowScanVisitor {
    fn visit_file(&mut self, node: &'ast File) {
        self.check_attrs(&node.attrs);
        syn::visit::visit_file(self, node);
    }

    fn visit_item_mod(&mut self, node: &'ast ItemMod) {
        self.visit_mod(node);
    }

    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        self.fn_stack.push(node.sig.ident.to_string());
        self.check_attrs(&node.attrs);
        syn::visit::visit_item_fn(self, node);
        self.fn_stack.pop();
    }

    fn visit_item_struct(&mut self, node: &'ast syn::ItemStruct) {
        self.check_attrs(&node.attrs);
        syn::visit::visit_item_struct(self, node);
    }

    fn visit_item_enum(&mut self, node: &'ast syn::ItemEnum) {
        self.check_attrs(&node.attrs);
        syn::visit::visit_item_enum(self, node);
    }

    fn visit_item_trait(&mut self, node: &'ast syn::ItemTrait) {
        self.check_attrs(&node.attrs);
        syn::visit::visit_item_trait(self, node);
    }

    fn visit_item_const(&mut self, node: &'ast syn::ItemConst) {
        self.check_attrs(&node.attrs);
        syn::visit::visit_item_const(self, node);
    }

    fn visit_item_static(&mut self, node: &'ast syn::ItemStatic) {
        self.check_attrs(&node.attrs);
        syn::visit::visit_item_static(self, node);
    }

    fn visit_item_type(&mut self, node: &'ast syn::ItemType) {
        self.check_attrs(&node.attrs);
        syn::visit::visit_item_type(self, node);
    }

    fn visit_item_impl(&mut self, node: &'ast ItemImpl) {
        self.check_attrs(&node.attrs);
        let prev = self.impl_type.clone();
        self.impl_type = Some(type_label(&node.self_ty));
        syn::visit::visit_item_impl(self, node);
        self.impl_type = prev;
    }

    fn visit_impl_item_fn(&mut self, node: &'ast ImplItemFn) {
        self.fn_stack.push(node.sig.ident.to_string());
        self.check_attrs(&node.attrs);
        syn::visit::visit_impl_item_fn(self, node);
        self.fn_stack.pop();
    }

    fn visit_field(&mut self, node: &'ast Field) {
        self.check_attrs(&node.attrs);
        syn::visit::visit_field(self, node);
    }

    fn visit_variant(&mut self, node: &'ast Variant) {
        self.check_attrs(&node.attrs);
        syn::visit::visit_variant(self, node);
    }

    fn visit_expr(&mut self, node: &'ast Expr) {
        if let Expr::Closure(closure) = node {
            self.check_attrs(&closure.attrs);
        }
        syn::visit::visit_expr(self, node);
    }
}

fn allow_snippet(attr: &Attribute) -> Option<String> {
    match &attr.meta {
        Meta::List(list) if list.path.is_ident("allow") => Some(truncate_snippet(
            &normalize_allow_tokens(&list.tokens.to_string()),
            96,
        )),
        _ => None,
    }
}

fn normalize_allow_tokens(text: &str) -> String {
    let collapsed = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace(" :: ", "::");
    if collapsed.is_empty() {
        "allow".to_string()
    } else {
        format!("allow({collapsed})")
    }
}

fn is_cfg_test(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        let Meta::List(list) = &attr.meta else {
            return false;
        };
        if !list.path.is_ident("cfg") {
            return false;
        }
        list.tokens.to_string().replace(' ', "") == "test"
    })
}

fn truncate_snippet(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max).collect();
    format!("{truncated}…")
}

fn type_label(ty: &Type) -> String {
    match ty {
        Type::Path(type_path) => path_label(&type_path.path),
        Type::Reference(reference) => type_label(&reference.elem),
        Type::Paren(paren) => type_label(&paren.elem),
        Type::Group(group) => type_label(&group.elem),
        _ => "?".to_string(),
    }
}

fn path_label(path: &syn::Path) -> String {
    path.segments
        .last()
        .map(|segment| segment.ident.to_string())
        .unwrap_or_else(|| "?".to_string())
}

//! syn-based scan for `#[allow(...)]` attributes.

use std::path::{Path, PathBuf};

use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{
    Attribute, Expr, ImplItemFn, ItemFn, ItemImpl, ItemMod, ItemUse, Meta, Token, Type, UseTree,
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

    #[instrument(level = "debug", skip(self, attr))]
    fn record_allow(&mut self, attr: &Attribute, use_targets: &[String]) {
        let Some(parsed) = parse_allow_attr(attr) else {
            return;
        };
        let Some(rule_id) = classify_allow(&parsed, use_targets) else {
            return;
        };
        let mut file = self.file.clone();
        if let Ok(rel) = file.strip_prefix(&self.crate_root) {
            file = rel.to_path_buf();
        }
        self.findings.push(AllowSiteRecord {
            rule_id,
            context: self.site_context(),
            file,
            line: attr.span().start().line as u32,
            snippet: parsed.snippet,
        });
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
        for attr in &item_mod.attrs {
            self.record_allow(attr, &[]);
        }
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
    #[instrument(level = "debug", skip(self, node))]
    fn visit_attribute(&mut self, node: &'ast Attribute) {
        self.record_allow(node, &[]);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_use(&mut self, node: &'ast ItemUse) {
        let targets = use_tree_paths(&node.tree);
        for attr in &node.attrs {
            self.record_allow(attr, &targets);
        }
        syn::visit::visit_use_tree(self, &node.tree);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_mod(&mut self, node: &'ast ItemMod) {
        self.visit_mod(node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        self.fn_stack.push(node.sig.ident.to_string());
        syn::visit::visit_item_fn(self, node);
        self.fn_stack.pop();
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_impl(&mut self, node: &'ast ItemImpl) {
        let prev = self.impl_type.clone();
        self.impl_type = Some(type_label(&node.self_ty));
        syn::visit::visit_item_impl(self, node);
        self.impl_type = prev;
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_impl_item_fn(&mut self, node: &'ast ImplItemFn) {
        self.fn_stack.push(node.sig.ident.to_string());
        syn::visit::visit_impl_item_fn(self, node);
        self.fn_stack.pop();
    }
}

struct ParsedAllow {
    snippet: String,
    reason: Option<String>,
}

#[instrument(level = "debug", skip(attr))]
fn parse_allow_attr(attr: &Attribute) -> Option<ParsedAllow> {
    let Meta::List(list) = &attr.meta else {
        return None;
    };
    if !list.path.is_ident("allow") {
        return None;
    }
    let snippet = truncate_snippet(&normalize_allow_tokens(&list.tokens.to_string()), 96);
    let metas = Punctuated::<Meta, Token![,]>::parse_terminated
        .parse2(list.tokens.clone())
        .ok()?;
    let mut reason = None;
    for meta in metas {
        let Meta::NameValue(name_value) = meta else {
            continue;
        };
        if !name_value.path.is_ident("reason") {
            continue;
        }
        let Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(text),
            ..
        }) = name_value.value
        else {
            continue;
        };
        reason = Some(text.value());
    }
    Some(ParsedAllow { snippet, reason })
}

#[instrument(level = "debug", skip(parsed, use_targets))]
fn classify_allow(parsed: &ParsedAllow, use_targets: &[String]) -> Option<AllowRuleId> {
    if !use_targets.iter().any(|path| is_verus_import(path)) {
        return Some(AllowRuleId::Attr001);
    }
    if parsed
        .reason
        .as_deref()
        .is_some_and(|reason| !reason.trim().is_empty())
    {
        return None;
    }
    Some(AllowRuleId::VerusReason001)
}

#[instrument(level = "debug")]
fn is_verus_import(path: &str) -> bool {
    path.split("::")
        .next()
        .is_some_and(|root| matches!(root, "vstd" | "verus_builtin" | "verus_builtin_macros"))
}

#[instrument(level = "debug", skip(tree))]
fn use_tree_paths(tree: &UseTree) -> Vec<String> {
    let mut paths = Vec::new();
    collect_use_tree_paths(tree, "", &mut paths);
    paths
}

#[instrument(level = "trace", skip(tree, out))]
fn collect_use_tree_paths(tree: &UseTree, prefix: &str, out: &mut Vec<String>) {
    match tree {
        UseTree::Path(path) => {
            let next = join_use_path(prefix, &path.ident.to_string());
            collect_use_tree_paths(&path.tree, &next, out);
        }
        UseTree::Name(name) => out.push(join_use_path(prefix, &name.ident.to_string())),
        UseTree::Rename(rename) => out.push(join_use_path(prefix, &rename.ident.to_string())),
        UseTree::Glob(_) => out.push(join_use_path(prefix, "*")),
        UseTree::Group(group) => {
            for item in &group.items {
                collect_use_tree_paths(item, prefix, out);
            }
        }
    }
}

#[instrument(level = "trace")]
fn join_use_path(prefix: &str, segment: &str) -> String {
    if prefix.is_empty() {
        segment.to_string()
    } else {
        format!("{prefix}::{segment}")
    }
}

#[instrument(level = "debug")]
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

#[instrument(level = "trace", skip(attrs))]
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

#[instrument(level = "debug")]
fn truncate_snippet(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max).collect();
    format!("{truncated}…")
}

#[instrument(level = "debug", skip(ty))]
fn type_label(ty: &Type) -> String {
    match ty {
        Type::Path(type_path) => path_label(&type_path.path),
        Type::Reference(reference) => type_label(&reference.elem),
        Type::Paren(paren) => type_label(&paren.elem),
        Type::Group(group) => type_label(&group.elem),
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

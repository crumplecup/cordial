//! syn-based scan for panic, unreachable, expect, unwrap, and compile_error sites.

use std::path::{Path, PathBuf};

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{
    Expr, ExprLit, ExprMacro, ExprMethodCall, Item, ItemFn, ItemImpl, ItemMod, Lit, Macro, Type,
};

use super::types::{PanicKind, PanicSiteRecord};
use crate::error::CordialResult;
use crate::loader::{module_path_from_src_file, path_has_fixtures, quality_scan_trees};

use tracing::instrument;
/// Scan `src/` and `tests/` under `crate_root`, excluding `fixtures/` paths.
#[instrument(level = "debug", err(level = "warn"))]
pub fn scan_crate_panics(crate_root: &Path) -> CordialResult<Vec<PanicSiteRecord>> {
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
pub fn scan_source_tree(src_root: &Path, crate_root: &Path) -> CordialResult<Vec<PanicSiteRecord>> {
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
        if path.extension().is_none_or(|ext| ext != "rs") || path_has_fixtures(path, crate_root) {
            continue;
        }
        let source = std::fs::read_to_string(path)?;
        findings.extend(scan_rust_source(&source, path, src_root, crate_root)?);
    }

    Ok(findings)
}

#[instrument(level = "debug", skip(source, file), err(level = "warn"))]
pub fn scan_rust_source(
    source: &str,
    file: &Path,
    src_root: &Path,
    crate_root: &Path,
) -> CordialResult<Vec<PanicSiteRecord>> {
    let syntax = syn::parse_file(source)
        .map_err(|err| crate::error::CordialError::syn_parse(file.display().to_string(), err))?;
    let module_prefix = module_path_from_src_file(src_root, file);
    let mut visitor = PanicScanVisitor {
        file: file.to_path_buf(),
        crate_root: crate_root.to_path_buf(),
        module_prefix,
        impl_type: None,
        fn_stack: Vec::new(),
        in_cfg_test: false,
        findings: Vec::new(),
    };
    visitor.visit_file(&syntax);
    Ok(visitor.findings)
}

struct PanicScanVisitor {
    file: PathBuf,
    crate_root: PathBuf,
    module_prefix: Vec<String>,
    impl_type: Option<String>,
    fn_stack: Vec<String>,
    in_cfg_test: bool,
    findings: Vec<PanicSiteRecord>,
}

impl PanicScanVisitor {
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

    fn push_finding(&mut self, kind: PanicKind, line: u32, snippet: String) {
        let mut file = self.file.clone();
        if let Ok(rel) = file.strip_prefix(&self.crate_root) {
            file = rel.to_path_buf();
        }
        if kind == PanicKind::Unwrap
            && self.findings.last().is_some_and(|last| {
                last.kind == PanicKind::Unwrap && last.line == line && last.file == file
            })
        {
            return;
        }
        self.findings.push(PanicSiteRecord {
            kind,
            context: self.site_context(),
            file,
            line,
            snippet,
            cfg_test: self.in_cfg_test,
        });
    }

    fn check_macro(&mut self, mac: &Macro) {
        let Some(kind) = macro_panic_kind(&mac.path) else {
            return;
        };
        self.push_finding(kind, mac.span().start().line as u32, macro_snippet(mac));
    }

    fn check_method_call(&mut self, call: &ExprMethodCall) {
        match call.method.to_string().as_str() {
            "expect" => self.push_finding(
                PanicKind::Expect,
                call.span().start().line as u32,
                expect_snippet(call),
            ),
            "unwrap" if !is_unwrap_variant(call) => self.push_finding(
                PanicKind::Unwrap,
                call.span().start().line as u32,
                ".unwrap()".to_string(),
            ),
            _ => {}
        }
    }

    fn visit_module_items(&mut self, items: &[Item], module_prefix: &[String]) {
        let prev_prefix = self.module_prefix.clone();
        self.module_prefix = module_prefix.to_vec();
        for item in items {
            syn::visit::visit_item(self, item);
        }
        self.module_prefix = prev_prefix;
    }

    fn visit_mod(&mut self, item_mod: &ItemMod) {
        let prev = self.in_cfg_test;
        if is_cfg_test(&item_mod.attrs) {
            self.in_cfg_test = true;
        }
        let Some((_, items)) = &item_mod.content else {
            self.in_cfg_test = prev;
            return;
        };
        let mut nested = self.module_prefix.clone();
        nested.push(item_mod.ident.to_string());
        self.visit_module_items(items, &nested);
        self.in_cfg_test = prev;
    }
}

impl<'ast> Visit<'ast> for PanicScanVisitor {
    fn visit_item_mod(&mut self, node: &'ast ItemMod) {
        self.visit_mod(node);
    }

    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        let prev = self.in_cfg_test;
        if is_cfg_test(&node.attrs) {
            self.in_cfg_test = true;
        }
        self.fn_stack.push(node.sig.ident.to_string());
        syn::visit::visit_item_fn(self, node);
        self.fn_stack.pop();
        self.in_cfg_test = prev;
    }

    fn visit_item_impl(&mut self, node: &'ast ItemImpl) {
        let prev_cfg = self.in_cfg_test;
        if is_cfg_test(&node.attrs) {
            self.in_cfg_test = true;
        }
        let prev = self.impl_type.clone();
        self.impl_type = Some(type_label(&node.self_ty));
        syn::visit::visit_item_impl(self, node);
        self.impl_type = prev;
        self.in_cfg_test = prev_cfg;
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        let prev = self.in_cfg_test;
        if is_cfg_test(&node.attrs) {
            self.in_cfg_test = true;
        }
        self.fn_stack.push(node.sig.ident.to_string());
        syn::visit::visit_impl_item_fn(self, node);
        self.fn_stack.pop();
        self.in_cfg_test = prev;
    }

    fn visit_stmt_macro(&mut self, node: &'ast syn::StmtMacro) {
        self.check_macro(&node.mac);
        syn::visit::visit_stmt_macro(self, node);
    }

    fn visit_item_macro(&mut self, node: &'ast syn::ItemMacro) {
        self.check_macro(&node.mac);
    }

    fn visit_expr_macro(&mut self, node: &'ast ExprMacro) {
        self.check_macro(&node.mac);
        syn::visit::visit_expr_macro(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
        self.check_method_call(node);
        syn::visit::visit_expr_method_call(self, node);
    }
}

fn is_unwrap_variant(call: &ExprMethodCall) -> bool {
    matches!(
        call.method.to_string().as_str(),
        "unwrap_or" | "unwrap_or_else" | "unwrap_or_default"
    )
}

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

fn macro_panic_kind(path: &syn::Path) -> Option<PanicKind> {
    let ident = path.segments.last()?.ident.to_string();
    match ident.as_str() {
        "panic" => Some(PanicKind::Panic),
        "unreachable" => Some(PanicKind::Unreachable),
        "compile_error" => Some(PanicKind::CompileError),
        _ => None,
    }
}

fn macro_snippet(mac: &Macro) -> String {
    let name = path_label(&mac.path);
    let args = mac.tokens.to_string();
    let trimmed = truncate_snippet(&args, 72);
    format!("{name}!({trimmed})")
}

fn expect_snippet(call: &ExprMethodCall) -> String {
    if let Some(Expr::Lit(ExprLit {
        lit: Lit::Str(lit), ..
    })) = call.args.first()
    {
        return format!(".expect(\"{}\")", lit.value());
    }
    ".expect(…)".to_string()
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

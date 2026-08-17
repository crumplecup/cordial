//! Unified syn visitor for error-handling IR scans (sites, chain, compliance).
//!
//! `error_sites` logic lives here unconditionally. `error_chain` and
//! `internal_error_chain` logic live in their own modules
//! (`chain_layer`, `compliance_layer`), each gated as a whole unit by a
//! single `#[cfg(feature = ...)]` on the `mod` declaration in
//! `error_ir/mod.rs`. This file only needs a handful of cfg attributes at
//! the boundary where it holds or calls into those layers — never inside a
//! shared helper function.

use std::path::{Path, PathBuf};

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{
    Expr, ExprCall, ExprIf, ExprMatch, ExprMethodCall, ExprReturn, ExprTry, ItemFn, ItemImpl,
    ItemMod, Pat, Type,
};

#[cfg(feature = "internal_error_chain")]
use super::compliance_layer::ComplianceLayer;
use crate::enricher::is_cfg_test;
use crate::etiquettes::error_sites::{ErrorSiteKind, ErrorSiteRecord};
#[cfg(feature = "internal_error_chain")]
use crate::etiquettes::internal_error_chain::{
    InternalErrorComplianceFinding, RawTypeNode, scan_error_rust_syntax_raw,
};
use crate::loader::module_path_from_src_file;
#[cfg(feature = "error_chain")]
use {super::chain_layer::ChainLayer, crate::etiquettes::error_chain::ErrorChainRecord};

/// Which error IR layers to collect during a unified file scan. Plain
/// `bool`s carry no feature-gated type, so they stay unconditional even
/// though only some are meaningful when a layer's feature is disabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErrorIrScanLayers {
    pub sites: bool,
    pub chain: bool,
    pub compliance: bool,
    pub type_graph: bool,
}

impl ErrorIrScanLayers {
    pub const SITES_ONLY: Self = Self {
        sites: true,
        chain: false,
        compliance: false,
        type_graph: false,
    };

    pub const CHAIN_ONLY: Self = Self {
        sites: false,
        chain: true,
        compliance: false,
        type_graph: false,
    };

    pub const COMPLIANCE_ONLY: Self = Self {
        sites: false,
        chain: false,
        compliance: true,
        type_graph: false,
    };

    #[allow(dead_code)]
    pub const FULL_SRC: Self = Self {
        sites: true,
        chain: true,
        compliance: true,
        type_graph: false,
    };

    pub fn for_unified_file(under_src: bool, under_error_module: bool) -> Self {
        Self {
            sites: true,
            chain: under_src,
            compliance: under_src,
            type_graph: under_error_module,
        }
    }
}

/// Combined scan output for one source file.
#[derive(Debug, Default)]
pub struct ErrorIrFileScan {
    pub sites: Vec<ErrorSiteRecord>,
    #[cfg(feature = "error_chain")]
    pub chain: Vec<ErrorChainRecord>,
    #[cfg(feature = "internal_error_chain")]
    pub compliance: Vec<InternalErrorComplianceFinding>,
    #[cfg(feature = "internal_error_chain")]
    pub type_graph_raw: Vec<RawTypeNode>,
}

/// Scan a pre-parsed file for error-handling IR facts (one AST walk for sites/chain/compliance).
pub fn scan_rust_file_syntax(
    syntax: &syn::File,
    file: &Path,
    tree_root: &Path,
    src_root: &Path,
    error_root: &Path,
    crate_root: &Path,
    crate_name: &str,
    layers: ErrorIrScanLayers,
) -> ErrorIrFileScan {
    let _ = src_root;
    let module_prefix = module_path_from_src_file(tree_root, file);
    let mut visitor = ErrorIrUnifiedVisitor {
        layers,
        crate_name: crate_name.to_string(),
        file: file.to_path_buf(),
        crate_root: crate_root.to_path_buf(),
        module_prefix,
        impl_type: None,
        fn_stack: Vec::new(),
        sites: Vec::new(),
        #[cfg(feature = "error_chain")]
        chain_layer: ChainLayer::new(),
        #[cfg(feature = "internal_error_chain")]
        compliance_layer: ComplianceLayer::new(),
    };
    visitor.visit_file(syntax);

    let mut scan = ErrorIrFileScan {
        sites: visitor.sites,
        #[cfg(feature = "error_chain")]
        chain: visitor.chain_layer.into_records(),
        #[cfg(feature = "internal_error_chain")]
        compliance: visitor.compliance_layer.into_findings(),
        #[cfg(feature = "internal_error_chain")]
        type_graph_raw: Vec::new(),
    };

    #[cfg(feature = "internal_error_chain")]
    if layers.type_graph {
        scan.type_graph_raw = scan_error_rust_syntax_raw(syntax, file, error_root);
    }

    scan
}

/// Anchoring context shared by the chain and compliance layers: where a
/// finding sits (module/fn path), and which file/crate it belongs to.
/// Plain data, no feature-gated types, so it lives here unconditionally
/// and both layer modules can depend on it freely. Unused (and allowed to
/// be so) when neither layer is compiled in.
#[allow(dead_code)]
pub(super) struct SiteCtx {
    pub context: String,
    pub rel_file: PathBuf,
    pub file: PathBuf,
    pub crate_name: String,
}

struct ErrorIrUnifiedVisitor {
    layers: ErrorIrScanLayers,
    crate_name: String,
    file: PathBuf,
    crate_root: PathBuf,
    module_prefix: Vec<String>,
    impl_type: Option<String>,
    fn_stack: Vec<String>,
    sites: Vec<ErrorSiteRecord>,
    #[cfg(feature = "error_chain")]
    chain_layer: ChainLayer,
    #[cfg(feature = "internal_error_chain")]
    compliance_layer: ComplianceLayer,
}

impl ErrorIrUnifiedVisitor {
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

    fn rel_file(&self) -> PathBuf {
        let mut file = self.file.clone();
        if let Ok(rel) = file.strip_prefix(&self.crate_root) {
            file = rel.to_path_buf();
        }
        file
    }

    #[cfg(any(feature = "error_chain", feature = "internal_error_chain"))]
    fn site_ctx(&self) -> SiteCtx {
        SiteCtx {
            context: self.site_context(),
            rel_file: self.rel_file(),
            file: self.file.clone(),
            crate_name: self.crate_name.clone(),
        }
    }

    fn push_site(&mut self, kind: ErrorSiteKind, line: u32, source: &Expr, site: String) {
        self.sites.push(ErrorSiteRecord {
            kind,
            context: self.site_context(),
            file: self.rel_file(),
            line,
            source_snippet: sites_expr_snippet(source),
            site_snippet: truncate_snippet(&site, 96),
        });
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

impl<'ast> Visit<'ast> for ErrorIrUnifiedVisitor {
    fn visit_item_mod(&mut self, node: &'ast ItemMod) {
        self.visit_mod(node);
    }

    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        #[cfg(feature = "error_chain")]
        let prev_return = self
            .layers
            .chain
            .then(|| self.chain_layer.enter_fn(&node.sig.output));
        self.fn_stack.push(node.sig.ident.to_string());
        syn::visit::visit_item_fn(self, node);
        self.fn_stack.pop();
        #[cfg(feature = "error_chain")]
        if let Some(prev) = prev_return {
            self.chain_layer.exit_fn(prev);
        }
    }

    fn visit_item_impl(&mut self, node: &'ast ItemImpl) {
        #[cfg(feature = "error_chain")]
        if self.layers.chain {
            let ctx = self.site_ctx();
            self.chain_layer.on_item_impl(node, &ctx);
        }
        let prev = self.impl_type.clone();
        self.impl_type = Some(impl_type_label(&node.self_ty));
        syn::visit::visit_item_impl(self, node);
        self.impl_type = prev;
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        #[cfg(feature = "error_chain")]
        let prev_return = self
            .layers
            .chain
            .then(|| self.chain_layer.enter_fn(&node.sig.output));
        self.fn_stack.push(node.sig.ident.to_string());
        syn::visit::visit_impl_item_fn(self, node);
        self.fn_stack.pop();
        #[cfg(feature = "error_chain")]
        if let Some(prev) = prev_return {
            self.chain_layer.exit_fn(prev);
        }
    }

    fn visit_item_struct(&mut self, node: &'ast syn::ItemStruct) {
        #[cfg(feature = "error_chain")]
        if self.layers.chain {
            let ctx = self.site_ctx();
            self.chain_layer.on_item_struct(node, &ctx);
        }
        syn::visit::visit_item_struct(self, node);
    }

    fn visit_item_enum(&mut self, node: &'ast syn::ItemEnum) {
        #[cfg(feature = "error_chain")]
        if self.layers.chain {
            let ctx = self.site_ctx();
            self.chain_layer.on_item_enum(node, &ctx);
        }
        syn::visit::visit_item_enum(self, node);
    }

    fn visit_expr_try(&mut self, node: &'ast ExprTry) {
        if self.layers.sites {
            let site = format!("{}?", sites_expr_snippet(&node.expr));
            self.push_site(
                ErrorSiteKind::QuestionMark,
                node.span().start().line as u32,
                &node.expr,
                truncate_snippet(&site, 96),
            );
        }

        #[cfg(feature = "error_chain")]
        if self.layers.chain {
            let ctx = self.site_ctx();
            self.chain_layer.on_expr_try(node, &ctx);
        }

        syn::visit::visit_expr_try(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
        if self.layers.sites {
            if node.method == "map_err" {
                let site = format!("{}.map_err(…)", sites_expr_snippet(&node.receiver));
                self.push_site(
                    ErrorSiteKind::MapErr,
                    node.span().start().line as u32,
                    &node.receiver,
                    truncate_snippet(&site, 96),
                );
            } else if node.method == "ok_or" || node.method == "ok_or_else" {
                let site = format!("{}.{}(…)", sites_expr_snippet(&node.receiver), node.method);
                self.push_site(
                    ErrorSiteKind::OkOr,
                    node.span().start().line as u32,
                    &node.receiver,
                    truncate_snippet(&site, 96),
                );
            }
        }

        #[cfg(feature = "error_chain")]
        if self.layers.chain && node.method == "map_err" {
            let ctx = self.site_ctx();
            self.chain_layer.on_map_err(node, &ctx);
        }

        #[cfg(feature = "internal_error_chain")]
        if self.layers.compliance
            && node.method == "map_err"
            && let Some(converter) = node.args.first()
        {
            let ctx = self.site_ctx();
            self.compliance_layer.on_map_err(
                &node.receiver,
                converter,
                node.span().start().line as u32,
                &ctx,
            );
        }

        syn::visit::visit_expr_method_call(self, node);
    }

    fn visit_expr_return(&mut self, node: &'ast ExprReturn) {
        if let Some(expr) = &node.expr {
            if self.layers.sites
                && let Some(inner) = sites_err_payload(expr)
            {
                self.push_site(
                    ErrorSiteKind::ReturnErr,
                    node.span().start().line as u32,
                    inner,
                    "return Err(…)".to_string(),
                );
            }
            #[cfg(feature = "internal_error_chain")]
            if self.layers.compliance {
                let ctx = self.site_ctx();
                self.compliance_layer
                    .on_return_err(expr, node.span().start().line as u32, &ctx);
            }
        }
        syn::visit::visit_expr_return(self, node);
    }

    fn visit_expr_if(&mut self, node: &'ast ExprIf) {
        if self.layers.sites
            && let Some(source) = if_let_err_source(&node.cond)
        {
            self.push_site(
                ErrorSiteKind::IfLetErr,
                node.cond.span().start().line as u32,
                source,
                format!("if let Err(…) = {}", sites_expr_snippet(source)),
            );
        }
        #[cfg(feature = "internal_error_chain")]
        if self.layers.compliance {
            let ctx = self.site_ctx();
            self.compliance_layer.on_if_let_err(node, &ctx);
        }
        syn::visit::visit_expr_if(self, node);
    }

    fn visit_expr_match(&mut self, node: &'ast ExprMatch) {
        if self.layers.sites && match_has_err_arm(&node.arms) {
            self.push_site(
                ErrorSiteKind::MatchErr,
                node.span().start().line as u32,
                &node.expr,
                format!("match {} {{ Err(…) => … }}", sites_expr_snippet(&node.expr)),
            );
        }
        #[cfg(feature = "internal_error_chain")]
        if self.layers.compliance {
            let ctx = self.site_ctx();
            self.compliance_layer.on_match_err(node, &ctx);
        }
        syn::visit::visit_expr_match(self, node);
    }
}

// --- sites helpers (shared: also used by chain/compliance snippet rendering) ---

fn sites_err_payload(expr: &Expr) -> Option<&Expr> {
    let Expr::Call(ExprCall { func, args, .. }) = expr else {
        return None;
    };
    if !expr_is_err(func) {
        return None;
    }
    args.first()
}

fn expr_is_err(expr: &Expr) -> bool {
    match expr {
        Expr::Path(path) => path.path.is_ident("Err"),
        _ => false,
    }
}

fn if_let_err_source(expr: &Expr) -> Option<&Expr> {
    let Expr::Let(let_expr) = expr else {
        return None;
    };
    if !pat_is_err(&let_expr.pat) {
        return None;
    }
    Some(&let_expr.expr)
}

pub(super) fn pat_is_err(pat: &Pat) -> bool {
    match pat {
        Pat::TupleStruct(tuple) => tuple.path.is_ident("Err"),
        Pat::Path(path) => path.path.is_ident("Err"),
        _ => false,
    }
}

fn match_has_err_arm(arms: &[syn::Arm]) -> bool {
    arms.iter().any(|arm| pat_is_err(&arm.pat))
}

fn sites_expr_snippet(expr: &Expr) -> String {
    truncate_snippet(&raw_expr_snippet(expr), 72)
}

// --- shared helpers (used across sites/chain/compliance) ---

pub(super) fn raw_expr_snippet(expr: &Expr) -> String {
    match expr {
        Expr::Macro(mac) => format!("{}!(…)", macro_path_label(&mac.mac.path)),
        Expr::Call(call) => format!("{}(…)", expr_path_label(&call.func)),
        Expr::MethodCall(method) => {
            format!(
                "{}.{}(…)",
                raw_expr_snippet(&method.receiver),
                method.method
            )
        }
        Expr::Path(path) => path
            .path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect::<Vec<_>>()
            .join("::"),
        Expr::Field(field) => format!(
            "{}.{}",
            raw_expr_snippet(&field.base),
            member_label(&field.member)
        ),
        Expr::Try(try_expr) => format!("{}?", raw_expr_snippet(&try_expr.expr)),
        _ => "…".to_string(),
    }
}

fn expr_path_label(expr: &Expr) -> String {
    match expr {
        Expr::Path(path) => path
            .path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect::<Vec<_>>()
            .join("::"),
        Expr::Field(field) => format!(
            "{}.{}",
            raw_expr_snippet(&field.base),
            member_label(&field.member)
        ),
        _ => "…".to_string(),
    }
}

fn macro_path_label(path: &syn::Path) -> String {
    path.get_ident()
        .map(syn::Ident::to_string)
        .unwrap_or_else(|| path_label(path))
}

fn member_label(member: &syn::Member) -> String {
    match member {
        syn::Member::Named(ident) => ident.to_string(),
        syn::Member::Unnamed(index) => index.index.to_string(),
    }
}

pub(super) fn truncate_snippet(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max).collect();
    format!("{truncated}…")
}

fn impl_type_label(ty: &Type) -> String {
    match ty {
        Type::Path(type_path) => path_label(&type_path.path),
        Type::Reference(reference) => impl_type_label(&reference.elem),
        Type::Paren(paren) => impl_type_label(&paren.elem),
        Type::Group(group) => impl_type_label(&group.elem),
        _ => "?".to_string(),
    }
}

pub(super) fn path_label(path: &syn::Path) -> String {
    path.segments
        .last()
        .map(|segment| segment.ident.to_string())
        .unwrap_or_else(|| "?".to_string())
}

//! Walk a parsed file and collect sites plus gated chain/compliance facts.

use std::path::PathBuf;

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{
    Expr, ExprIf, ExprMatch, ExprMethodCall, ExprReturn, ExprTry, ItemFn, ItemImpl, ItemMod,
};

use crate::enricher::is_cfg_test;
#[cfg(feature = "error_chain")]
use crate::etiquettes::error_ir::chain_layer::ChainLayer;
#[cfg(feature = "internal_error_chain")]
use crate::etiquettes::error_ir::compliance_layer::ComplianceLayer;
use crate::etiquettes::error_sites::{ErrorSiteKind, ErrorSiteRecord};

use super::ErrorIrScanLayers;
use super::expr::{
    if_let_err_source, impl_type_label, match_has_err_arm, sites_err_payload, sites_expr_snippet,
    truncate_snippet,
};
use super::site::SiteCtx;

pub(super) struct ErrorIrUnifiedVisitor {
    pub(super) layers: ErrorIrScanLayers,
    pub(super) crate_name: String,
    pub(super) file: PathBuf,
    pub(super) crate_root: PathBuf,
    pub(super) module_prefix: Vec<String>,
    pub(super) impl_type: Option<String>,
    pub(super) fn_stack: Vec<String>,
    pub(super) sites: Vec<ErrorSiteRecord>,
    #[cfg(feature = "error_chain")]
    pub(super) chain_layer: ChainLayer,
    #[cfg(feature = "internal_error_chain")]
    pub(super) compliance_layer: ComplianceLayer,
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

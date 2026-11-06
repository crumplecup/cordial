//! Walk a shared compatibility tree for chain and compliance facts.

use std::path::PathBuf;

#[cfg(feature = "internal_error_chain")]
use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{ExprIf, ExprMatch, ExprMethodCall, ExprReturn, ExprTry, ItemFn, ItemImpl, ItemMod};

use crate::enricher::is_cfg_test;
use crate::error::CordialResult;
#[cfg(feature = "error_chain")]
use crate::etiquettes::error_ir::chain_layer::ChainLayer;
#[cfg(feature = "internal_error_chain")]
use crate::etiquettes::error_ir::compliance_layer::ComplianceLayer;
use tracing::instrument;

use super::ErrorIrScanLayers;
use super::expr::impl_type_label;
pub(super) struct ErrorIrUnifiedVisitor {
    layers: ErrorIrScanLayers,
    #[cfg(feature = "internal_error_chain")]
    crate_name: String,
    file: PathBuf,
    crate_root: PathBuf,
    module_prefix: Vec<String>,
    impl_type: Option<String>,
    fn_stack: Vec<String>,
    #[cfg(feature = "error_chain")]
    chain_layer: ChainLayer,
    #[cfg(feature = "internal_error_chain")]
    compliance_layer: ComplianceLayer,
    error: Option<crate::error::CordialError>,
}

impl ErrorIrUnifiedVisitor {
    #[instrument(level = "debug", skip(layers, file), fields(crate_name = crate_name))]
    pub(super) fn new(
        layers: ErrorIrScanLayers,
        crate_name: String,
        file: PathBuf,
        crate_root: PathBuf,
        module_prefix: Vec<String>,
    ) -> Self {
        Self {
            layers,
            #[cfg(feature = "internal_error_chain")]
            crate_name,
            file,
            crate_root,
            module_prefix,
            impl_type: None,
            fn_stack: Vec::new(),
            #[cfg(feature = "error_chain")]
            chain_layer: ChainLayer::new(),
            #[cfg(feature = "internal_error_chain")]
            compliance_layer: ComplianceLayer::new(layers.is_proc_macro_crate()),
            error: None,
        }
    }

    #[instrument(level = "debug", skip(self))]
    pub(super) fn into_file_scan(self) -> CordialResult<super::ErrorIrFileScan> {
        if let Some(error) = self.error {
            return Err(error);
        }
        Ok(super::ErrorIrFileScan::from_parts(
            #[cfg(feature = "error_chain")]
            self.chain_layer.into_records()?,
            #[cfg(feature = "internal_error_chain")]
            self.compliance_layer.into_findings()?,
        ))
    }

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

    #[instrument(level = "debug", skip(self))]
    fn rel_file(&self) -> PathBuf {
        let mut file = self.file.clone();
        if let Ok(rel) = file.strip_prefix(&self.crate_root) {
            file = rel.to_path_buf();
        }
        file
    }

    #[instrument(level = "trace", skip(self))]
    #[cfg(any(feature = "error_chain", feature = "internal_error_chain"))]
    fn site_ctx(&self) -> CordialResult<super::site::SiteCtx> {
        let mut builder = super::site::SiteCtx::builder();
        builder
            .context(self.site_context())
            .rel_file(self.rel_file());
        #[cfg(feature = "internal_error_chain")]
        builder
            .file(self.file.clone())
            .crate_name(self.crate_name.clone());
        builder.build()
    }

    #[instrument(level = "trace", skip(self))]
    #[cfg(any(feature = "error_chain", feature = "internal_error_chain"))]
    fn site_ctx_or_record(&mut self) -> Option<super::site::SiteCtx> {
        if self.error.is_some() {
            return None;
        }
        match self.site_ctx() {
            Ok(ctx) => Some(ctx),
            Err(error) => {
                self.error = Some(error);
                None
            }
        }
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

impl<'ast> Visit<'ast> for ErrorIrUnifiedVisitor {
    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_mod(&mut self, node: &'ast ItemMod) {
        self.visit_mod(node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        #[cfg(feature = "error_chain")]
        let prev_return = self
            .layers
            .chain()
            .then(|| self.chain_layer.enter_fn(&node.sig.output));
        self.fn_stack.push(node.sig.ident.to_string());
        syn::visit::visit_item_fn(self, node);
        self.fn_stack.pop();
        #[cfg(feature = "error_chain")]
        if let Some(prev) = prev_return {
            self.chain_layer.exit_fn(prev);
        }
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_impl(&mut self, node: &'ast ItemImpl) {
        #[cfg(feature = "error_chain")]
        if self.layers.chain()
            && let Some(ctx) = self.site_ctx_or_record()
        {
            self.chain_layer.on_item_impl(node, &ctx);
        }
        let prev = self.impl_type.clone();
        self.impl_type = Some(impl_type_label(&node.self_ty));
        syn::visit::visit_item_impl(self, node);
        self.impl_type = prev;
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        #[cfg(feature = "error_chain")]
        let prev_return = self
            .layers
            .chain()
            .then(|| self.chain_layer.enter_fn(&node.sig.output));
        self.fn_stack.push(node.sig.ident.to_string());
        syn::visit::visit_impl_item_fn(self, node);
        self.fn_stack.pop();
        #[cfg(feature = "error_chain")]
        if let Some(prev) = prev_return {
            self.chain_layer.exit_fn(prev);
        }
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_struct(&mut self, node: &'ast syn::ItemStruct) {
        #[cfg(feature = "error_chain")]
        if self.layers.chain()
            && let Some(ctx) = self.site_ctx_or_record()
        {
            self.chain_layer.on_item_struct(node, &ctx);
        }
        syn::visit::visit_item_struct(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_enum(&mut self, node: &'ast syn::ItemEnum) {
        #[cfg(feature = "error_chain")]
        if self.layers.chain()
            && let Some(ctx) = self.site_ctx_or_record()
        {
            self.chain_layer.on_item_enum(node, &ctx);
        }
        syn::visit::visit_item_enum(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_expr_try(&mut self, node: &'ast ExprTry) {
        #[cfg(feature = "error_chain")]
        if self.layers.chain()
            && let Some(ctx) = self.site_ctx_or_record()
        {
            self.chain_layer.on_expr_try(node, &ctx);
        }

        syn::visit::visit_expr_try(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
        #[cfg(feature = "error_chain")]
        if self.layers.chain()
            && node.method == "map_err"
            && let Some(ctx) = self.site_ctx_or_record()
        {
            self.chain_layer.on_map_err(node, &ctx);
        }

        #[cfg(feature = "internal_error_chain")]
        if self.layers.compliance()
            && node.method == "map_err"
            && let Some(converter) = node.args.first()
            && let Some(ctx) = self.site_ctx_or_record()
        {
            self.compliance_layer.on_map_err(
                &node.receiver,
                converter,
                node.span().start().line as u32,
                &ctx,
            );
        }

        syn::visit::visit_expr_method_call(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_expr_return(&mut self, node: &'ast ExprReturn) {
        #[cfg(feature = "internal_error_chain")]
        if let Some(expr) = &node.expr
            && self.layers.compliance()
            && let Some(ctx) = self.site_ctx_or_record()
        {
            self.compliance_layer
                .on_return_err(expr, node.span().start().line as u32, &ctx);
        }
        syn::visit::visit_expr_return(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_expr_if(&mut self, node: &'ast ExprIf) {
        #[cfg(feature = "internal_error_chain")]
        if self.layers.compliance()
            && let Some(ctx) = self.site_ctx_or_record()
        {
            self.compliance_layer.on_if_let_err(node, &ctx);
        }
        syn::visit::visit_expr_if(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_expr_match(&mut self, node: &'ast ExprMatch) {
        #[cfg(feature = "internal_error_chain")]
        if self.layers.compliance()
            && let Some(ctx) = self.site_ctx_or_record()
        {
            self.compliance_layer.on_match_err(node, &ctx);
        }
        syn::visit::visit_expr_match(self, node);
    }
}

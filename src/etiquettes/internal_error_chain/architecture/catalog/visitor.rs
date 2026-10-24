use std::path::PathBuf;

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{Expr, ExprCall, ExprMethodCall, ItemEnum, ItemFn, ItemImpl, ItemMod, ItemStruct};
use tracing::instrument;

use crate::enricher::is_cfg_test;
use crate::error::CordialResult;

use super::fanout::{
    call_owner_and_name, direct_source_target, direct_target, literal_string, wrapper_target,
};
use super::{Catalog, FanoutCall, FanoutWrapper, MapErrSite};

mod record;

#[derive(derive_builder::Builder, derive_setters::Setters)]
#[builder(
    name = "CatalogVisitorBuilder",
    pattern = "owned",
    build_fn(name = "build_inner", private, error = "crate::error::CordialError")
)]
#[setters(generate = false, borrow_self)]
pub(in crate::etiquettes::internal_error_chain::architecture) struct CatalogVisitor<'a> {
    file: PathBuf,
    module_prefix: Vec<String>,
    catalog: &'a mut Catalog,
    #[builder(default)]
    inside_kind_call: bool,
}

impl<'a> CatalogVisitor<'a> {
    #[instrument(level = "debug")]
    pub(in crate::etiquettes::internal_error_chain::architecture) fn builder()
    -> CatalogVisitorBuilder<'a> {
        CatalogVisitorBuilder::default()
    }
}

impl<'a> CatalogVisitorBuilder<'a> {
    #[instrument(level = "debug", skip(self))]
    pub(in crate::etiquettes::internal_error_chain::architecture) fn build(
        self,
    ) -> CordialResult<CatalogVisitor<'a>> {
        self.build_inner()
    }
}

impl<'ast> Visit<'ast> for CatalogVisitor<'_> {
    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        if is_cfg_test(&node.attrs) {
            return;
        }
        if let Some(target) = wrapper_target(&node.sig, &node.block) {
            self.catalog.fanout_wrappers.push(FanoutWrapper::new(
                self.file.clone(),
                None,
                node.sig.ident.to_string(),
                node.sig.ident.span().start().line as u32,
                target,
            ));
        }
        syn::visit::visit_item_fn(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_expr_call(&mut self, node: &'ast ExprCall) {
        let direct = direct_target(node);
        if let Some((target, literal)) = &direct {
            self.catalog.fanout_calls.push(FanoutCall::new(
                self.file.clone(),
                node.span().start().line as u32,
                None,
                String::new(),
                Some(target.clone()),
                literal.clone(),
            ));
        } else if !self.inside_kind_call
            && let Some((target, literal)) = direct_source_target(node)
        {
            self.catalog.fanout_calls.push(FanoutCall::new(
                self.file.clone(),
                node.span().start().line as u32,
                None,
                String::new(),
                Some(target),
                literal,
            ));
        } else if node.args.len() == 1
            && let (Some((owner, callee)), Some(literal)) = (
                call_owner_and_name(&node.func),
                node.args.first().and_then(literal_string),
            )
        {
            self.catalog.fanout_calls.push(FanoutCall::new(
                self.file.clone(),
                node.span().start().line as u32,
                owner,
                callee,
                None,
                literal,
            ));
        }
        let was_inside_kind_call = self.inside_kind_call;
        self.inside_kind_call |= direct.is_some();
        syn::visit::visit_expr_call(self, node);
        self.inside_kind_call = was_inside_kind_call;
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
        if node.method == "map_err"
            && let Some(Expr::Path(converter)) = node.args.first()
        {
            let segments: Vec<_> = converter.path.segments.iter().collect();
            if segments.len() >= 2 {
                let method = segments[segments.len() - 1].ident.to_string();
                let type_ident = segments[segments.len() - 2].ident.to_string();
                self.catalog.map_err_sites.push(MapErrSite {
                    type_ident,
                    method,
                    file: self.file.clone(),
                    line: node.method.span().start().line as u32,
                });
            }
        }
        syn::visit::visit_expr_method_call(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_mod(&mut self, node: &'ast ItemMod) {
        if is_cfg_test(&node.attrs) {
            return;
        }
        let Some((_, items)) = &node.content else {
            return;
        };
        let mut nested = self.module_prefix.clone();
        nested.push(node.ident.to_string());
        let prev = self.module_prefix.clone();
        self.module_prefix = nested;
        for item in items {
            syn::visit::visit_item(self, item);
        }
        self.module_prefix = prev;
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_struct(&mut self, node: &'ast ItemStruct) {
        self.record_struct(node);
        syn::visit::visit_item_struct(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_enum(&mut self, node: &'ast ItemEnum) {
        self.record_enum(node);
        syn::visit::visit_item_enum(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_impl(&mut self, node: &'ast ItemImpl) {
        self.record_impl(node);
        syn::visit::visit_item_impl(self, node);
    }
}

//! Syntax visitor for conversion-site evidence across a shared source tree.

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{
    Expr, ExprCall, ExprField, ExprStruct, ExprTry, ItemImpl, ItemType, ItemUse, Macro, Member,
    PatStruct, TypePath,
};
use tracing::instrument;

use super::super::attributes::call_site_attribute_is_stable;
use super::super::edges::error_edge_evidence;
use super::super::from::ErrorFromEvidence;
use super::super::helpers::{
    derive_is_nonconstructing, path_mentions, type_mentions, use_tree_mentions,
};
use super::super::result_alias::stable_result_alias;
use super::super::source_method::ErrorSourceMethodEvidence;
use super::super::try_conversion::ErrorTryScope;
use super::super::{ErrorConversionSiteKind, ErrorEdgeField};
use super::BatchInventoryVisitor;
use crate::etiquettes::internal_error_chain::apply::initializers::FieldKind;
use crate::etiquettes::internal_error_chain::apply::simple_type_name;
use crate::etiquettes::internal_error_chain::apply::uses::{
    macro_has_unhandled_field_use, tokens_access_field, tokens_construct_type,
};
use crate::etiquettes::internal_error_chain::type_graph::trait_is_std_error;

impl<'ast> Visit<'ast> for BatchInventoryVisitor<'_> {
    #[instrument(level = "debug", skip(self, item))]
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        self.module_depth += 1;
        syn::visit::visit_item_mod(self, item);
        self.module_depth -= 1;
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_item_impl(&mut self, item: &'ast ItemImpl) {
        let previous = std::mem::replace(&mut self.current_impl, simple_type_name(&item.self_ty));
        let previous_trait = std::mem::replace(
            &mut self.current_error_trait,
            item.trait_
                .as_ref()
                .map(|(path, _)| path)
                .filter(|path| trait_is_std_error(path))
                .cloned(),
        );
        if let Some((path, _)) = &item.trait_ {
            if let Some(evidence) = ErrorFromEvidence::from_impl(item) {
                for offset in 0..self.relevant.len() {
                    let index = self.relevant[offset];
                    let name = &self.candidates[index].type_name;
                    let own_impl = self.current_impl.as_ref() == Some(name);
                    if own_impl || path_mentions(path, name) {
                        self.record(
                            index,
                            ErrorConversionSiteKind::FromConversion,
                            item.span(),
                            false,
                        );
                        if let Some(site) = self.sites[index].last_mut() {
                            site.from_conversion = Some(evidence.clone());
                        }
                    }
                }
            }
        }
        syn::visit::visit_item_impl(self, item);
        self.current_impl = previous;
        self.current_error_trait = previous_trait;
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        let previous_source_sites = std::mem::take(&mut self.current_source_sites);
        if item.sig.ident == "source" {
            if let (Some(name), Some(trait_path)) = (
                self.current_impl.as_ref(),
                self.current_error_trait.as_ref(),
            ) {
                let name = name.clone();
                let evidence = ErrorSourceMethodEvidence::from_method(self.file, item, trait_path);
                for offset in 0..self.relevant.len() {
                    let index = self.relevant[offset];
                    if self.candidates[index].type_name == name {
                        self.record(
                            index,
                            ErrorConversionSiteKind::SourceMethod,
                            item.span(),
                            false,
                        );
                        let site_index = self.sites[index].len() - 1;
                        self.sites[index][site_index].source_method = Some(evidence.clone());
                        self.current_source_sites.push((index, site_index));
                    }
                }
            }
        }
        let previous = self.enter_return_scope(Some(&item.sig.output), ErrorTryScope::Function);
        syn::visit::visit_impl_item_fn(self, item);
        self.leave_return_scope(previous);
        self.current_source_sites = previous_source_sites;
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        let previous = self.enter_return_scope(Some(&item.sig.output), ErrorTryScope::Function);
        syn::visit::visit_item_fn(self, item);
        self.leave_return_scope(previous);
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_expr_closure(&mut self, item: &'ast syn::ExprClosure) {
        let (output, scope) = match &item.output {
            syn::ReturnType::Default => (None, ErrorTryScope::InferredClosure),
            output => (Some(output), ErrorTryScope::Closure),
        };
        let previous = self.enter_return_scope(output, scope);
        syn::visit::visit_expr_closure(self, item);
        self.leave_return_scope(previous);
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_expr_async(&mut self, item: &'ast syn::ExprAsync) {
        let previous = self.enter_return_scope(None, ErrorTryScope::AsyncBlock);
        syn::visit::visit_expr_async(self, item);
        self.leave_return_scope(previous);
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_expr_try_block(&mut self, item: &'ast syn::ExprTryBlock) {
        let previous = self.enter_return_scope(None, ErrorTryScope::TryBlock);
        syn::visit::visit_expr_try_block(self, item);
        self.leave_return_scope(previous);
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_expr_struct(&mut self, item: &'ast ExprStruct) {
        for offset in 0..self.relevant.len() {
            let index = self.relevant[offset];
            if self.direct_type(index, &item.path) {
                self.record_path(
                    index,
                    ErrorConversionSiteKind::Constructor,
                    item.span(),
                    item.rest.is_none(),
                    &item.path,
                    false,
                );
            } else if self.matches_type(index, &item.path) {
                self.record_path(
                    index,
                    ErrorConversionSiteKind::Constructor,
                    item.span(),
                    false,
                    &item.path,
                    false,
                );
            }
        }
        syn::visit::visit_expr_struct(self, item);
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_expr_call(&mut self, item: &'ast ExprCall) {
        if let Expr::Path(path) = &*item.func {
            let mut matched = false;
            for offset in 0..self.relevant.len() {
                let index = self.relevant[offset];
                if self.associated_path(index, &path.path) {
                    self.record_path(
                        index,
                        ErrorConversionSiteKind::Caller,
                        item.span(),
                        path.qself.is_none() && path.path.segments.len() == 2,
                        &path.path,
                        true,
                    );
                    matched = true;
                }
            }
            if matched {
                for arg in &item.args {
                    self.visit_expr(arg);
                }
                return;
            }
        }
        syn::visit::visit_expr_call(self, item);
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_expr_path(&mut self, item: &'ast syn::ExprPath) {
        for offset in 0..self.relevant.len() {
            let index = self.relevant[offset];
            if self.associated_path(index, &item.path) {
                self.record_path(
                    index,
                    ErrorConversionSiteKind::Alias,
                    item.span(),
                    false,
                    &item.path,
                    true,
                );
            }
        }
        syn::visit::visit_expr_path(self, item);
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_expr_field(&mut self, item: &'ast ExprField) {
        if let Member::Named(name) = &item.member {
            if matches!(&*item.base, Expr::Path(path) if path.qself.is_none() && path.path.is_ident("self"))
            {
                for &(index, site_index) in &self.current_source_sites {
                    if let Some(evidence) = self.sites[index][site_index].source_method.as_mut() {
                        evidence.record_self_field(name);
                    }
                }
            }
            for offset in 0..self.relevant.len() {
                let index = self.relevant[offset];
                let candidate = &self.candidates[index];
                if name == candidate.field_name.as_str()
                    && !(candidate.field.getter_storage.is_some()
                        && self.declarations.unrelated_getter_field(
                            &candidate.declaration,
                            &candidate.type_name,
                            self.file.path(),
                            item.span(),
                        ))
                {
                    self.record(index, ErrorConversionSiteKind::FieldUse, item.span(), false);
                }
            }
        }
        syn::visit::visit_expr_field(self, item);
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_pat_struct(&mut self, item: &'ast PatStruct) {
        for offset in 0..self.relevant.len() {
            let index = self.relevant[offset];
            if (self.direct_type(index, &item.path) || self.matches_type(index, &item.path))
                && item.fields.iter().any(|field| matches!(&field.member, Member::Named(name) if name == self.candidates[index].field_name.as_str()))
            {
                self.record_path(
                    index,
                    ErrorConversionSiteKind::FieldPattern,
                    item.span(),
                    false,
                    &item.path,
                    false,
                );
            }
        }
        syn::visit::visit_pat_struct(self, item);
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_expr_try(&mut self, item: &'ast ExprTry) {
        for offset in 0..self.relevant.len() {
            let index = self.relevant[offset];
            if self.returning_error[index] {
                self.record(
                    index,
                    ErrorConversionSiteKind::TryConversion,
                    item.span(),
                    false,
                );
                if let Some(site) = self.sites[index].last_mut() {
                    site.try_conversion = self.try_evidence.clone();
                }
            }
        }
        syn::visit::visit_expr_try(self, item);
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_type_path(&mut self, item: &'ast TypePath) {
        for offset in 0..self.relevant.len() {
            let index = self.relevant[offset];
            if self.matches_type(index, &item.path) {
                self.record_path(
                    index,
                    ErrorConversionSiteKind::TypeUse,
                    item.span(),
                    item.qself.is_none() && item.path.segments.len() == 1,
                    &item.path,
                    false,
                );
            }
        }
        syn::visit::visit_type_path(self, item);
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_item_type(&mut self, item: &'ast ItemType) {
        for offset in 0..self.relevant.len() {
            let index = self.relevant[offset];
            let name = &self.candidates[index].type_name;
            if type_mentions(&item.ty, name) || item.ident == *name {
                let resolved = self.module_depth == 0
                    && self.candidates[index].field.kind == FieldKind::CallSiteFile
                    && stable_result_alias(item, self.file.syntax(), name);
                self.record(index, ErrorConversionSiteKind::Alias, item.span(), resolved);
            }
        }
        syn::visit::visit_item_type(self, item);
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_item_use(&mut self, item: &'ast ItemUse) {
        for offset in 0..self.relevant.len() {
            let index = self.relevant[offset];
            if use_tree_mentions(&item.tree, &self.candidates[index].type_name) {
                self.record(index, ErrorConversionSiteKind::Alias, item.span(), false);
            }
        }
        syn::visit::visit_item_use(self, item);
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_macro(&mut self, item: &'ast Macro) {
        for offset in 0..self.relevant.len() {
            let index = self.relevant[offset];
            let candidate = &self.candidates[index];
            let constructs = tokens_construct_type(&item.tokens, &candidate.type_name);
            if constructs || tokens_access_field(&item.tokens, &candidate.field_name) {
                if !constructs
                    && (candidate.field.getter_storage.is_some()
                        && self.declarations.unrelated_getter_macro(
                            &candidate.declaration,
                            &candidate.type_name,
                            self.file.path(),
                            item.span(),
                        ))
                {
                    continue;
                }
                let resolved =
                    !constructs && !macro_has_unhandled_field_use(item, &candidate.field_name);
                self.record(index, ErrorConversionSiteKind::Macro, item.span(), resolved);
            }
        }
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_item_struct(&mut self, item: &'ast syn::ItemStruct) {
        for offset in 0..self.relevant.len() {
            let index = self.relevant[offset];
            if item.ident != self.candidates[index].type_name {
                continue;
            }
            let candidate = &self.candidates[index];
            let field_name = candidate.field_name.clone();
            let call_site = candidate.field.kind == FieldKind::CallSiteFile;
            for attr in &item.attrs {
                if call_site
                    && call_site_attribute_is_stable(
                        attr,
                        &item.attrs,
                        &field_name,
                        self.file.syntax(),
                    )
                {
                    continue;
                }
                if attr.path().is_ident("derive") {
                    if !derive_is_nonconstructing(attr) {
                        self.record(
                            index,
                            ErrorConversionSiteKind::GeneratedConstructor,
                            attr.span(),
                            false,
                        );
                    }
                } else if !matches!(
                    attr.path().get_ident().map(ToString::to_string).as_deref(),
                    Some("allow" | "warn" | "deny" | "forbid" | "cfg" | "doc" | "repr")
                ) {
                    self.record(
                        index,
                        ErrorConversionSiteKind::GeneratedConstructor,
                        attr.span(),
                        false,
                    );
                }
            }
            for field in &item.fields {
                if let Some(edge_field) =
                    field
                        .ident
                        .as_ref()
                        .and_then(|name| match name.to_string().as_str() {
                            "source" => Some(ErrorEdgeField::Source),
                            "kind" => Some(ErrorEdgeField::Kind),
                            _ => None,
                        })
                {
                    let edge = error_edge_evidence(self.file, field, edge_field);
                    self.record(
                        index,
                        ErrorConversionSiteKind::ErrorEdge,
                        field.span(),
                        edge.is_some(),
                    );
                    if let Some(site) = self.sites[index].last_mut() {
                        site.edge = edge;
                    }
                }
            }
        }
        syn::visit::visit_item_struct(self, item);
    }
}

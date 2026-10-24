//! One syntax walk per source file, dispatching sites to candidate fields.

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{
    Expr, ExprCall, ExprField, ExprStruct, ExprTry, ItemImpl, ItemType, ItemUse, Macro, Member,
    PatStruct, TypePath,
};
use tracing::instrument;

use crate::loader::SharedParsedFile;

use super::helpers::{path_mentions, return_type_mentions, type_mentions, use_tree_mentions};
use super::{ConversionCandidate, ErrorConversionSite, ErrorConversionSiteKind};
use crate::etiquettes::internal_error_chain::apply::uses::{
    macro_has_unhandled_field_use, tokens_access_field, tokens_construct_type,
};
use crate::etiquettes::internal_error_chain::apply::{simple_type_name, span_range};

pub(super) struct BatchInventoryVisitor<'a> {
    file: &'a SharedParsedFile,
    candidates: &'a [ConversionCandidate],
    relevant: &'a [usize],
    module_depth: usize,
    current_impl: Option<String>,
    returning_error: Vec<bool>,
    sites: Vec<Vec<ErrorConversionSite>>,
}

impl<'a> BatchInventoryVisitor<'a> {
    #[instrument(level = "debug", skip(file, candidates, relevant))]
    pub(super) fn collect(
        file: &'a SharedParsedFile,
        candidates: &'a [ConversionCandidate],
        relevant: &'a [usize],
    ) -> Vec<Vec<ErrorConversionSite>> {
        let mut visitor = Self {
            file,
            candidates,
            relevant,
            module_depth: 0,
            current_impl: None,
            returning_error: vec![false; candidates.len()],
            sites: vec![Vec::new(); candidates.len()],
        };
        visitor.visit_file(file.syntax());
        visitor.sites
    }

    #[instrument(level = "debug", skip(self, kind, span))]
    fn record(
        &mut self,
        index: usize,
        kind: ErrorConversionSiteKind,
        span: proc_macro2::Span,
        resolved: bool,
    ) {
        let start = span.start();
        self.sites[index].push(ErrorConversionSite {
            path: self.file.path().to_path_buf(),
            line: start.line,
            column: start.column,
            kind,
            related_type: None,
            resolved: resolved
                && self.file.path() == self.candidates[index].declaration
                && self.module_depth == 0,
        });
    }

    #[instrument(level = "trace", skip(self, path))]
    fn matches_type(&self, index: usize, path: &syn::Path) -> bool {
        path.segments
            .last()
            .is_some_and(|part| part.ident == self.candidates[index].type_name)
    }

    #[instrument(level = "trace", skip(self, path))]
    fn direct_type(&self, index: usize, path: &syn::Path) -> bool {
        let name = &self.candidates[index].type_name;
        path.is_ident(name) || (path.is_ident("Self") && self.current_impl.as_ref() == Some(name))
    }

    #[instrument(level = "trace", skip(self, path))]
    fn associated_path(&self, index: usize, path: &syn::Path) -> bool {
        let name = &self.candidates[index].type_name;
        path.segments.len() > 1
            && path.segments.first().is_some_and(|part| {
                part.ident == *name
                    || (part.ident == "Self" && self.current_impl.as_ref() == Some(name))
            })
    }
}

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
        if let Some((path, _)) = &item.trait_ {
            if path
                .segments
                .last()
                .is_some_and(|part| part.ident == "From")
            {
                for offset in 0..self.relevant.len() {
                    let index = self.relevant[offset];
                    let name = &self.candidates[index].type_name;
                    let own_impl = self.current_impl.as_ref() == Some(name);
                    if own_impl || path_mentions(path, name) {
                        self.record(
                            index,
                            ErrorConversionSiteKind::FromConversion,
                            item.span(),
                            own_impl,
                        );
                    }
                }
            }
        }
        syn::visit::visit_item_impl(self, item);
        self.current_impl = previous;
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        let previous = self.returning_error.clone();
        for &index in self.relevant {
            let name = &self.candidates[index].type_name;
            self.returning_error[index] = self.current_impl.as_ref() == Some(name)
                || return_type_mentions(&item.sig.output, name);
        }
        syn::visit::visit_impl_item_fn(self, item);
        self.returning_error = previous;
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        let previous = self.returning_error.clone();
        for &index in self.relevant {
            self.returning_error[index] =
                return_type_mentions(&item.sig.output, &self.candidates[index].type_name);
        }
        syn::visit::visit_item_fn(self, item);
        self.returning_error = previous;
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_expr_struct(&mut self, item: &'ast ExprStruct) {
        for offset in 0..self.relevant.len() {
            let index = self.relevant[offset];
            if self.direct_type(index, &item.path) {
                self.record(
                    index,
                    ErrorConversionSiteKind::Constructor,
                    item.span(),
                    item.rest.is_none(),
                );
            } else if self.matches_type(index, &item.path) {
                self.record(
                    index,
                    ErrorConversionSiteKind::Constructor,
                    item.span(),
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
                    self.record(
                        index,
                        ErrorConversionSiteKind::Caller,
                        item.span(),
                        path.qself.is_none() && path.path.segments.len() == 2,
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
                self.record(index, ErrorConversionSiteKind::Alias, item.span(), false);
            }
        }
        syn::visit::visit_expr_path(self, item);
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_expr_field(&mut self, item: &'ast ExprField) {
        if let Member::Named(name) = &item.member {
            for offset in 0..self.relevant.len() {
                let index = self.relevant[offset];
                if name == self.candidates[index].field_name.as_str() {
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
                self.record(index, ErrorConversionSiteKind::FieldPattern, item.span(), false);
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
            }
        }
        syn::visit::visit_expr_try(self, item);
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_type_path(&mut self, item: &'ast TypePath) {
        for offset in 0..self.relevant.len() {
            let index = self.relevant[offset];
            if self.matches_type(index, &item.path) {
                self.record(
                    index,
                    ErrorConversionSiteKind::TypeUse,
                    item.span(),
                    item.qself.is_none() && item.path.segments.len() == 1,
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
                self.record(index, ErrorConversionSiteKind::Alias, item.span(), false);
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
            for attr in &item.attrs {
                if attr.path().is_ident("derive") {
                    let mut opaque = false;
                    if attr
                        .parse_nested_meta(|meta| {
                            opaque |= !meta.path.segments.last().is_some_and(|part| {
                                matches!(
                                    part.ident.to_string().as_str(),
                                    "Debug"
                                        | "Clone"
                                        | "Copy"
                                        | "PartialEq"
                                        | "Eq"
                                        | "Hash"
                                        | "PartialOrd"
                                        | "Ord"
                                        | "Error"
                                )
                            });
                            Ok(())
                        })
                        .is_err()
                    {
                        opaque = true;
                    }
                    if opaque {
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
                if field
                    .ident
                    .as_ref()
                    .is_some_and(|name| name == "source" || name == "kind")
                {
                    self.record(
                        index,
                        ErrorConversionSiteKind::ErrorEdge,
                        field.span(),
                        true,
                    );
                    if let Some(site) = self.sites[index].last_mut() {
                        site.related_type = span_range(self.file.source(), field.ty.span())
                            .map(|(start, end)| self.file.source()[start..end].to_string());
                    }
                }
            }
        }
        syn::visit::visit_item_struct(self, item);
    }
}

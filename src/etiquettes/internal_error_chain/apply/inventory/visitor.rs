//! Visit shared syntax snapshots to locate conversion sites.

use std::path::Path;

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{
    Expr, ExprCall, ExprField, ExprStruct, ExprTry, ItemImpl, ItemType, ItemUse, Macro, Member,
    PatStruct, TypePath,
};
use tracing::instrument;

use crate::loader::SharedParsedFile;

use super::super::uses::{
    macro_has_unhandled_field_use, tokens_access_field, tokens_construct_type,
};
use super::super::{simple_type_name, source_has_ident, span_range};
use super::helpers::{path_mentions, return_type_mentions, type_mentions, use_tree_mentions};
use super::{ErrorConversionSite, ErrorConversionSiteKind, ErrorConversionUnit};

impl ErrorConversionUnit {
    #[instrument(level = "debug", skip(files))]
    pub(super) fn collect(
        files: &[SharedParsedFile],
        declaration: &Path,
        type_name: &str,
        field_name: &str,
        name_ambiguous: bool,
    ) -> Self {
        let mut sites = Vec::new();
        for file in files {
            let external = file.path() != declaration;
            if external && !source_has_ident(file.source(), type_name) {
                continue;
            }
            let mut visitor = InventoryVisitor {
                path: file.path(),
                source: file.source(),
                type_name,
                field_name,
                external,
                module_depth: 0,
                current_impl: None,
                returning_error: false,
                sites: Vec::new(),
            };
            visitor.visit_file(file.syntax());
            if !external
                && name_ambiguous
                && !visitor
                    .sites
                    .iter()
                    .any(|site| site.kind == ErrorConversionSiteKind::Alias && !site.resolved)
            {
                visitor.sites.push(ErrorConversionSite {
                    path: file.path().to_path_buf(),
                    line: 0,
                    column: 0,
                    kind: ErrorConversionSiteKind::Alias,
                    related_type: None,
                    resolved: false,
                });
            }
            if external && visitor.sites.is_empty() && source_has_ident(file.source(), type_name) {
                visitor.sites.push(ErrorConversionSite {
                    path: file.path().to_path_buf(),
                    line: 0,
                    column: 0,
                    kind: ErrorConversionSiteKind::CrossFileReference,
                    related_type: None,
                    resolved: false,
                });
            }
            sites.extend(visitor.sites);
        }
        Self {
            declaration: declaration.to_path_buf(),
            type_name: type_name.to_string(),
            field_name: field_name.to_string(),
            sites,
        }
    }

    /// Whether every recorded path is understood well enough to plan a unit.
    #[instrument(level = "trace", skip(self))]
    pub fn is_closed(&self) -> bool {
        self.sites.iter().all(|site| site.resolved)
            && self
                .sites
                .iter()
                .any(|site| site.kind == ErrorConversionSiteKind::Constructor)
    }

    #[instrument(level = "trace", skip(self))]
    pub(in crate::etiquettes::internal_error_chain::apply) fn blocks_existing_field_edit(
        &self,
    ) -> bool {
        self.sites.iter().any(|site| {
            (!site.resolved && site.path != self.declaration)
                || (!site.resolved
                    && matches!(
                        site.kind,
                        ErrorConversionSiteKind::FieldUse
                            | ErrorConversionSiteKind::FieldPattern
                            | ErrorConversionSiteKind::Alias
                            | ErrorConversionSiteKind::Macro
                            | ErrorConversionSiteKind::CrossFileReference
                            | ErrorConversionSiteKind::Constructor
                            | ErrorConversionSiteKind::GeneratedConstructor
                    ))
        })
    }
}

struct InventoryVisitor<'a> {
    path: &'a Path,
    source: &'a str,
    type_name: &'a str,
    field_name: &'a str,
    external: bool,
    module_depth: usize,
    current_impl: Option<String>,
    returning_error: bool,
    sites: Vec<ErrorConversionSite>,
}

impl InventoryVisitor<'_> {
    #[instrument(level = "debug", skip(self, kind, span))]
    fn record(&mut self, kind: ErrorConversionSiteKind, span: proc_macro2::Span, resolved: bool) {
        let start = span.start();
        self.sites.push(ErrorConversionSite {
            path: self.path.to_path_buf(),
            line: start.line,
            column: start.column,
            kind,
            related_type: None,
            resolved: resolved && !self.external && self.module_depth == 0,
        });
    }

    #[instrument(level = "trace", skip(self, path))]
    fn matches_type(&self, path: &syn::Path) -> bool {
        path.segments
            .last()
            .is_some_and(|part| part.ident == self.type_name)
    }

    #[instrument(level = "trace", skip(self, path))]
    fn direct_type(&self, path: &syn::Path) -> bool {
        path.is_ident(self.type_name)
            || (path.is_ident("Self") && self.current_impl.as_deref() == Some(self.type_name))
    }

    #[instrument(level = "trace", skip(self, path))]
    fn associated_path(&self, path: &syn::Path) -> bool {
        path.segments.len() > 1
            && path.segments.first().is_some_and(|part| {
                part.ident == self.type_name
                    || (part.ident == "Self"
                        && self.current_impl.as_deref() == Some(self.type_name))
            })
    }
}

impl<'ast> Visit<'ast> for InventoryVisitor<'_> {
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
                && (self.current_impl.as_deref() == Some(self.type_name)
                    || path_mentions(path, self.type_name))
            {
                self.record(
                    ErrorConversionSiteKind::FromConversion,
                    item.span(),
                    self.current_impl.as_deref() == Some(self.type_name),
                );
            }
        }
        syn::visit::visit_item_impl(self, item);
        self.current_impl = previous;
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        let previous = std::mem::replace(
            &mut self.returning_error,
            self.current_impl.as_deref() == Some(self.type_name)
                || return_type_mentions(&item.sig.output, self.type_name),
        );
        syn::visit::visit_impl_item_fn(self, item);
        self.returning_error = previous;
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        let previous = std::mem::replace(
            &mut self.returning_error,
            return_type_mentions(&item.sig.output, self.type_name),
        );
        syn::visit::visit_item_fn(self, item);
        self.returning_error = previous;
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_expr_struct(&mut self, item: &'ast ExprStruct) {
        if self.direct_type(&item.path) {
            self.record(
                ErrorConversionSiteKind::Constructor,
                item.span(),
                item.rest.is_none(),
            );
        } else if self.matches_type(&item.path) {
            self.record(ErrorConversionSiteKind::Constructor, item.span(), false);
        }
        syn::visit::visit_expr_struct(self, item);
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_expr_call(&mut self, item: &'ast ExprCall) {
        if let Expr::Path(path) = &*item.func {
            if self.associated_path(&path.path) {
                self.record(
                    ErrorConversionSiteKind::Caller,
                    item.span(),
                    path.qself.is_none() && path.path.segments.len() == 2,
                );
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
        if self.associated_path(&item.path) {
            self.record(ErrorConversionSiteKind::Alias, item.span(), false);
        }
        syn::visit::visit_expr_path(self, item);
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_expr_field(&mut self, item: &'ast ExprField) {
        if matches!(&item.member, Member::Named(name) if name == self.field_name) {
            self.record(ErrorConversionSiteKind::FieldUse, item.span(), false);
        }
        syn::visit::visit_expr_field(self, item);
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_pat_struct(&mut self, item: &'ast PatStruct) {
        if self.direct_type(&item.path) || self.matches_type(&item.path) {
            if item.fields.iter().any(
                |field| matches!(&field.member, Member::Named(name) if name == self.field_name),
            ) {
                self.record(ErrorConversionSiteKind::FieldPattern, item.span(), false);
            }
        }
        syn::visit::visit_pat_struct(self, item);
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_expr_try(&mut self, item: &'ast ExprTry) {
        if self.returning_error {
            self.record(ErrorConversionSiteKind::TryConversion, item.span(), false);
        }
        syn::visit::visit_expr_try(self, item);
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_type_path(&mut self, item: &'ast TypePath) {
        if self.matches_type(&item.path) {
            self.record(
                ErrorConversionSiteKind::TypeUse,
                item.span(),
                item.qself.is_none() && item.path.segments.len() == 1,
            );
        }
        syn::visit::visit_type_path(self, item);
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_item_type(&mut self, item: &'ast ItemType) {
        if type_mentions(&item.ty, self.type_name) || item.ident == self.type_name {
            self.record(ErrorConversionSiteKind::Alias, item.span(), false);
        }
        syn::visit::visit_item_type(self, item);
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_item_use(&mut self, item: &'ast ItemUse) {
        if use_tree_mentions(&item.tree, self.type_name) {
            self.record(ErrorConversionSiteKind::Alias, item.span(), false);
        }
        syn::visit::visit_item_use(self, item);
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_macro(&mut self, item: &'ast Macro) {
        if tokens_construct_type(&item.tokens, self.type_name)
            || tokens_access_field(&item.tokens, self.field_name)
        {
            self.record(
                ErrorConversionSiteKind::Macro,
                item.span(),
                !tokens_construct_type(&item.tokens, self.type_name)
                    && !macro_has_unhandled_field_use(item, self.field_name),
            );
        }
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_item_struct(&mut self, item: &'ast syn::ItemStruct) {
        if item.ident == self.type_name {
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
                    self.record(ErrorConversionSiteKind::ErrorEdge, field.span(), true);
                    if let Some(site) = self.sites.last_mut() {
                        site.related_type = span_range(self.source, field.ty.span())
                            .map(|(start, end)| self.source[start..end].to_string());
                    }
                }
            }
        }
        syn::visit::visit_item_struct(self, item);
    }
}

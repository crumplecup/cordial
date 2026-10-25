use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{GenericParam, ItemEnum, ItemStruct, PathArguments, Type};
use tracing::instrument;

use crate::config::ErrorFieldStrategy;
use crate::loader::SharedParsedFile;

use super::super::super::{Edit, span_range};
use super::super::types::type_name;
use super::provenance::{returns_graph_type, safe_impl_constructor, safe_top_level_return};
use super::{GraphLifetimePlan, LifetimeUse, one_lifetime};

#[instrument(level = "debug", skip(plan, file, strategy, storage_edits))]
pub(super) fn collect(
    plan: &GraphLifetimePlan,
    file: &SharedParsedFile,
    strategy: ErrorFieldStrategy,
    storage_edits: &[Edit],
) -> Option<Vec<Edit>> {
    let mut visitor = LifetimeEdits {
        file,
        strategy,
        plan,
        storage_edits,
        edits: Vec::new(),
        valid: true,
        scope: false,
        target_impl: false,
        impl_moves_lifetime: false,
        module_depth: 0,
    };
    visitor.visit_file(file.syntax());
    visitor.valid.then_some(visitor.edits)
}

struct LifetimeEdits<'a> {
    file: &'a SharedParsedFile,
    strategy: ErrorFieldStrategy,
    plan: &'a GraphLifetimePlan,
    storage_edits: &'a [Edit],
    edits: Vec<Edit>,
    valid: bool,
    scope: bool,
    target_impl: bool,
    impl_moves_lifetime: bool,
    module_depth: usize,
}

impl LifetimeEdits<'_> {
    #[instrument(level = "debug", skip(self, span))]
    fn edit(&mut self, span: proc_macro2::Span, replacement: String) {
        if let Some((start, end)) = span_range(self.file.source(), span) {
            self.edits.push(Edit {
                start,
                end,
                replacement,
            });
        } else {
            self.valid = false;
        }
    }

    #[instrument(level = "debug", skip(self, span))]
    fn insert_after(&mut self, span: proc_macro2::Span, replacement: String) {
        if let Some((_, end)) = span_range(self.file.source(), span) {
            self.edits.push(Edit {
                start: end,
                end,
                replacement,
            });
        } else {
            self.valid = false;
        }
    }

    #[instrument(level = "trace", skip(self, span))]
    fn is_covered(&self, span: proc_macro2::Span) -> bool {
        span_range(self.file.source(), span).is_some_and(|(start, end)| {
            self.storage_edits
                .iter()
                .any(|edit| edit.start <= start && end <= edit.end && edit.start < edit.end)
        })
    }
}

impl<'ast> Visit<'ast> for LifetimeEdits<'_> {
    #[instrument(level = "trace", skip(self, item))]
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        self.module_depth += 1;
        syn::visit::visit_item_mod(self, item);
        self.module_depth -= 1;
    }

    #[instrument(level = "trace", skip(self, item))]
    fn visit_item_struct(&mut self, item: &'ast ItemStruct) {
        let previous = self.scope;
        if self.plan.names.contains(&item.ident.to_string()) {
            if self.module_depth > 0 {
                self.valid = false;
                return;
            }
            self.scope = true;
            match self.strategy {
                ErrorFieldStrategy::Borrowed if item.generics.params.is_empty() => {
                    self.insert_after(item.ident.span(), format!("<'{}>", self.plan.lifetime))
                }
                ErrorFieldStrategy::Owned
                    if one_lifetime(&item.generics).as_deref()
                        == Some(self.plan.lifetime().as_str()) =>
                {
                    self.edit(item.generics.span(), String::new());
                }
                _ => self.valid = false,
            }
        }
        syn::visit::visit_item_struct(self, item);
        self.scope = previous;
    }

    #[instrument(level = "trace", skip(self, item))]
    fn visit_item_enum(&mut self, item: &'ast ItemEnum) {
        let previous = self.scope;
        if self.plan.names.contains(&item.ident.to_string()) {
            if self.module_depth > 0 {
                self.valid = false;
                return;
            }
            self.scope = true;
            match self.strategy {
                ErrorFieldStrategy::Borrowed if item.generics.params.is_empty() => {
                    self.insert_after(item.ident.span(), format!("<'{}>", self.plan.lifetime))
                }
                ErrorFieldStrategy::Owned
                    if one_lifetime(&item.generics).as_deref()
                        == Some(self.plan.lifetime().as_str()) =>
                {
                    self.edit(item.generics.span(), String::new());
                }
                _ => self.valid = false,
            }
        }
        syn::visit::visit_item_enum(self, item);
        self.scope = previous;
    }

    #[instrument(level = "trace", skip(self, item))]
    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        let previous_scope = self.scope;
        let previous_target = self.target_impl;
        let previous_moves = self.impl_moves_lifetime;
        self.target_impl =
            type_name(&item.self_ty).is_some_and(|name| self.plan.names.contains(&name));
        if self.target_impl && self.module_depth > 0 {
            self.valid = false;
            return;
        }
        self.impl_moves_lifetime = false;
        self.scope = one_lifetime(&item.generics).as_deref() == Some(self.plan.lifetime().as_str());
        if self.target_impl {
            match self.strategy {
                ErrorFieldStrategy::Borrowed if item.generics.params.is_empty() => {
                    self.insert_after(item.impl_token.span(), format!("<'{}>", self.plan.lifetime));
                    self.scope = true;
                }
                ErrorFieldStrategy::Borrowed if self.scope => {}
                ErrorFieldStrategy::Owned if self.scope => {
                    let trait_uses_lifetime = item.trait_.as_ref().is_some_and(|(path, _)| {
                        let mut visitor = LifetimeUse {
                            lifetime: self.plan.lifetime().as_str(),
                            found: false,
                        };
                        visitor.visit_path(path);
                        visitor.found
                    });
                    if !trait_uses_lifetime {
                        self.edit(item.generics.span(), String::new());
                        self.impl_moves_lifetime = true;
                    }
                }
                _ => self.valid = false,
            }
        }
        syn::visit::visit_item_impl(self, item);
        self.scope = previous_scope;
        self.target_impl = previous_target;
        self.impl_moves_lifetime = previous_moves;
    }

    #[instrument(level = "trace", skip(self, item))]
    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        if self.target_impl {
            if self.strategy == ErrorFieldStrategy::Borrowed
                && matches!(&item.sig.output, syn::ReturnType::Type(_, ty)
                    if matches!(&**ty, Type::Path(path) if path.path.is_ident("Self")))
                && !safe_impl_constructor(item, self.plan.lifetime().as_str())
            {
                self.valid = false;
            }
            match self.strategy {
                ErrorFieldStrategy::Borrowed => {
                    if item.sig.generics.params.is_empty() {
                        // The new impl lifetime is already in scope.
                    } else if one_lifetime(&item.sig.generics).as_deref()
                        == Some(self.plan.lifetime().as_str())
                        && matches!(&item.sig.output, syn::ReturnType::Type(_, ty)
                            if matches!(&**ty, Type::Path(path) if path.path.is_ident("Self")))
                        && item
                            .sig
                            .inputs
                            .iter()
                            .all(|input| matches!(input, syn::FnArg::Typed(_)))
                    {
                        self.edit(item.sig.generics.span(), String::new());
                    } else {
                        self.valid = false;
                    }
                }
                ErrorFieldStrategy::Owned if self.impl_moves_lifetime => {
                    let mut lifetime_use = LifetimeUse {
                        lifetime: self.plan.lifetime().as_str(),
                        found: false,
                    };
                    lifetime_use.visit_signature(&item.sig);
                    lifetime_use.visit_block(&item.block);
                    if lifetime_use.found {
                        if item.sig.generics.params.is_empty() {
                            self.insert_after(
                                item.sig.ident.span(),
                                format!("<'{}>", self.plan.lifetime().as_str()),
                            );
                        } else {
                            self.valid = false;
                        }
                    }
                }
                _ => {}
            }
        }
        syn::visit::visit_impl_item_fn(self, item);
    }

    #[instrument(level = "trace", skip(self, item))]
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        let previous = self.scope;
        self.scope = item.sig.generics.params.iter().any(|param| {
            matches!(param, GenericParam::Lifetime(lifetime)
                if lifetime.lifetime.ident == self.plan.lifetime().as_str())
        });
        if self.strategy == ErrorFieldStrategy::Borrowed
            && returns_graph_type(&item.sig.output, self.plan.names())
            && !safe_top_level_return(item, self.plan.names(), self.plan.lifetime().as_str())
        {
            self.valid = false;
        }
        syn::visit::visit_item_fn(self, item);
        self.scope = previous;
    }

    #[instrument(level = "trace", skip(self, item))]
    fn visit_type_path(&mut self, item: &'ast syn::TypePath) {
        let segment = item.path.segments.last();
        if let Some(segment) = segment {
            if self.plan.names.contains(&segment.ident.to_string()) && !self.is_covered(item.span())
            {
                if !self.scope || item.qself.is_some() || item.path.segments.len() != 1 {
                    self.valid = false;
                } else {
                    match (&self.strategy, &segment.arguments) {
                        (ErrorFieldStrategy::Borrowed, PathArguments::None) => self.insert_after(
                            segment.ident.span(),
                            format!("<'{}>", self.plan.lifetime().as_str()),
                        ),
                        (ErrorFieldStrategy::Owned, PathArguments::AngleBracketed(arguments))
                            if arguments.args.len() == 1
                                && matches!(arguments.args.first(), Some(syn::GenericArgument::Lifetime(named))
                                    if named.ident == self.plan.lifetime().as_str()) =>
                        {
                            self.edit(arguments.span(), String::new());
                        }
                        _ => self.valid = false,
                    }
                }
            }
        }
        syn::visit::visit_type_path(self, item);
    }

    #[instrument(level = "trace", skip(self, item))]
    fn visit_expr_path(&mut self, item: &'ast syn::ExprPath) {
        if item.path.segments.iter().any(|segment| {
            self.plan.names.contains(&segment.ident.to_string())
                && !matches!(segment.arguments, PathArguments::None)
        }) {
            self.valid = false;
        }
        syn::visit::visit_expr_path(self, item);
    }
}

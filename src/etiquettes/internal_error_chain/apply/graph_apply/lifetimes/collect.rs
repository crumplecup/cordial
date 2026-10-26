//! Collect lifetime sites from the shared syntax tree.

use std::collections::{BTreeMap, BTreeSet};

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{GenericParam, PathArguments, Type};
use tracing::instrument;

use crate::loader::SharedParsedFile;

use super::super::super::span_range;
use super::super::types::type_name;
use super::one_lifetime;
use super::provenance::{safe_impl_constructor, safe_top_level_return};
use super::{
    ConditionalScope, Declaration, FunctionSite, GraphLifetimeFacts, ImplSite, MethodSite, Range,
    TypeArguments, TypePathSite,
};

trait VisitTarget<'ast> {
    fn visit_with(&'ast self, visitor: &mut LifetimeNames);
}

impl GraphLifetimeFacts {
    #[instrument(level = "debug", skip(file, tracked_names))]
    pub(in crate::etiquettes::internal_error_chain::apply) fn collect(
        file: &SharedParsedFile,
        tracked_names: &BTreeSet<String>,
    ) -> Self {
        let mut collector = Collector {
            file,
            tracked_names,
            facts: Self::default(),
            module_depth: 0,
            scope_lifetimes: BTreeSet::new(),
            conditional_scope: None,
        };
        collector.visit_file(file.syntax());
        collector.facts
    }
}

struct Collector<'a> {
    file: &'a SharedParsedFile,
    tracked_names: &'a BTreeSet<String>,
    facts: GraphLifetimeFacts,
    module_depth: usize,
    scope_lifetimes: BTreeSet<String>,
    conditional_scope: Option<ConditionalScope>,
}

impl Collector<'_> {
    #[instrument(level = "trace", skip(self, span))]
    fn range(&self, span: proc_macro2::Span) -> Option<Range> {
        span_range(self.file.source(), span)
    }

    #[instrument(level = "trace", skip(self, span))]
    fn end(&self, span: proc_macro2::Span) -> Option<usize> {
        self.range(span).map(|(_, end)| end)
    }
}

impl<'ast> Visit<'ast> for Collector<'_> {
    #[instrument(level = "trace", skip(self, item))]
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        self.module_depth += 1;
        syn::visit::visit_item_mod(self, item);
        self.module_depth -= 1;
    }

    #[instrument(level = "trace", skip(self, item))]
    fn visit_item_struct(&mut self, item: &'ast syn::ItemStruct) {
        let name = item.ident.to_string();
        if self.tracked_names.contains(&name) {
            self.facts.declarations.push(Declaration {
                name: name.clone(),
                module_depth: self.module_depth,
                ident_end: self.end(item.ident.span()),
                generics_empty: item.generics.params.is_empty(),
                one_lifetime: one_lifetime(&item.generics),
                generics_range: self.range(item.generics.span()),
            });
        }
        let previous = self.conditional_scope.replace(ConditionalScope {
            owner: name,
            borrowed_only: false,
        });
        syn::visit::visit_item_struct(self, item);
        self.conditional_scope = previous;
    }

    #[instrument(level = "trace", skip(self, item))]
    fn visit_item_enum(&mut self, item: &'ast syn::ItemEnum) {
        let name = item.ident.to_string();
        if self.tracked_names.contains(&name) {
            self.facts.declarations.push(Declaration {
                name: name.clone(),
                module_depth: self.module_depth,
                ident_end: self.end(item.ident.span()),
                generics_empty: item.generics.params.is_empty(),
                one_lifetime: one_lifetime(&item.generics),
                generics_range: self.range(item.generics.span()),
            });
        }
        let previous = self.conditional_scope.replace(ConditionalScope {
            owner: name,
            borrowed_only: false,
        });
        syn::visit::visit_item_enum(self, item);
        self.conditional_scope = previous;
    }

    #[instrument(level = "trace", skip(self, item))]
    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        let name = type_name(&item.self_ty);
        let methods = item
            .items
            .iter()
            .filter(|_| {
                name.as_ref()
                    .is_some_and(|name| self.tracked_names.contains(name))
            })
            .filter_map(|member| match member {
                syn::ImplItem::Fn(method) => Some(method),
                _ => None,
            })
            .map(|method| {
                let lifetime_names = all_lifetimes(&method.sig);
                let safe_constructor_lifetimes = lifetime_names
                    .iter()
                    .filter(|lifetime| safe_impl_constructor(method, lifetime))
                    .cloned()
                    .collect();
                let mut used_lifetimes = lifetime_names;
                used_lifetimes.extend(all_lifetimes(&method.block));
                MethodSite {
                    output_self: matches!(&method.sig.output, syn::ReturnType::Type(_, ty)
                        if matches!(&**ty, Type::Path(path) if path.path.is_ident("Self"))),
                    all_inputs_typed: method
                        .sig
                        .inputs
                        .iter()
                        .all(|input| matches!(input, syn::FnArg::Typed(_))),
                    generics_empty: method.sig.generics.params.is_empty(),
                    one_lifetime: one_lifetime(&method.sig.generics),
                    generics_range: self.range(method.sig.generics.span()),
                    ident_end: self.end(method.sig.ident.span()),
                    safe_constructor_lifetimes,
                    used_lifetimes,
                }
            })
            .collect();
        let trait_lifetimes = item
            .trait_
            .as_ref()
            .map_or_else(BTreeSet::new, |(path, _)| all_lifetimes(path));
        self.facts.impls.push(ImplSite {
            name: name.clone(),
            module_depth: self.module_depth,
            impl_token_end: self.end(item.impl_token.span()),
            generics_empty: item.generics.params.is_empty(),
            one_lifetime: one_lifetime(&item.generics),
            generics_range: self.range(item.generics.span()),
            trait_lifetimes,
            methods,
        });
        let previous_scope = std::mem::replace(
            &mut self.scope_lifetimes,
            one_lifetime(&item.generics).into_iter().collect(),
        );
        let previous_conditional = std::mem::replace(
            &mut self.conditional_scope,
            name.filter(|_| item.generics.params.is_empty())
                .map(|owner| ConditionalScope {
                    owner,
                    borrowed_only: true,
                }),
        );
        syn::visit::visit_item_impl(self, item);
        self.scope_lifetimes = previous_scope;
        self.conditional_scope = previous_conditional;
    }

    #[instrument(level = "trace", skip(self, item))]
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        let return_names: BTreeSet<_> = return_type_names(&item.sig.output)
            .intersection(self.tracked_names)
            .cloned()
            .collect();
        let mut safe_returns = BTreeMap::new();
        for lifetime in all_lifetimes(&item.sig) {
            let safe: BTreeSet<_> = return_names
                .iter()
                .filter(|name| {
                    safe_top_level_return(item, &BTreeSet::from([(*name).clone()]), &lifetime)
                })
                .cloned()
                .collect();
            safe_returns.insert(lifetime, safe);
        }
        if !return_names.is_empty() {
            self.facts.functions.push(FunctionSite {
                return_names,
                safe_returns,
            });
        }
        let previous_scope = std::mem::replace(
            &mut self.scope_lifetimes,
            item.sig
                .generics
                .params
                .iter()
                .filter_map(|param| match param {
                    GenericParam::Lifetime(lifetime) => Some(lifetime.lifetime.ident.to_string()),
                    _ => None,
                })
                .collect(),
        );
        let previous_conditional = self.conditional_scope.take();
        syn::visit::visit_item_fn(self, item);
        self.scope_lifetimes = previous_scope;
        self.conditional_scope = previous_conditional;
    }

    #[instrument(level = "trace", skip(self, item))]
    fn visit_type_path(&mut self, item: &'ast syn::TypePath) {
        if let Some(segment) = item.path.segments.last()
            && self.tracked_names.contains(&segment.ident.to_string())
        {
            let arguments = match &segment.arguments {
                PathArguments::None => TypeArguments::None,
                PathArguments::AngleBracketed(arguments) if arguments.args.len() == 1 => {
                    match arguments.args.first() {
                        Some(syn::GenericArgument::Lifetime(lifetime)) => {
                            TypeArguments::OneLifetime {
                                name: lifetime.ident.to_string(),
                                range: self.range(arguments.span()),
                            }
                        }
                        _ => TypeArguments::Other,
                    }
                }
                _ => TypeArguments::Other,
            };
            self.facts.type_paths.push(TypePathSite {
                name: segment.ident.to_string(),
                range: self.range(item.span()),
                ident_end: self.end(segment.ident.span()),
                arguments,
                qself: item.qself.is_some(),
                path_len: item.path.segments.len(),
                scope_lifetimes: self.scope_lifetimes.clone(),
                conditional_scope: self.conditional_scope.clone(),
            });
        }
        syn::visit::visit_type_path(self, item);
    }

    #[instrument(level = "trace", skip(self, item))]
    fn visit_expr_path(&mut self, item: &'ast syn::ExprPath) {
        self.facts.expr_path_blockers.extend(
            item.path
                .segments
                .iter()
                .filter(|segment| {
                    self.tracked_names.contains(&segment.ident.to_string())
                        && !matches!(segment.arguments, PathArguments::None)
                })
                .map(|segment| segment.ident.to_string()),
        );
        syn::visit::visit_expr_path(self, item);
    }
}

#[instrument(level = "trace", skip(value))]
fn all_lifetimes<'ast, T: VisitTarget<'ast>>(value: &'ast T) -> BTreeSet<String> {
    let mut names = LifetimeNames(BTreeSet::new());
    value.visit_with(&mut names);
    names.0
}

impl<'ast> VisitTarget<'ast> for syn::Signature {
    #[instrument(level = "trace", skip(self, visitor))]
    fn visit_with(&'ast self, visitor: &mut LifetimeNames) {
        visitor.visit_signature(self);
    }
}

impl<'ast> VisitTarget<'ast> for syn::Block {
    #[instrument(level = "trace", skip(self, visitor))]
    fn visit_with(&'ast self, visitor: &mut LifetimeNames) {
        visitor.visit_block(self);
    }
}

impl<'ast> VisitTarget<'ast> for syn::Path {
    #[instrument(level = "trace", skip(self, visitor))]
    fn visit_with(&'ast self, visitor: &mut LifetimeNames) {
        visitor.visit_path(self);
    }
}

struct LifetimeNames(BTreeSet<String>);

impl<'ast> Visit<'ast> for LifetimeNames {
    #[instrument(level = "debug", skip(self, lifetime))]
    fn visit_lifetime(&mut self, lifetime: &'ast syn::Lifetime) {
        self.0.insert(lifetime.ident.to_string());
    }
}

#[instrument(level = "trace", skip(output))]
fn return_type_names(output: &syn::ReturnType) -> BTreeSet<String> {
    struct Names(BTreeSet<String>);
    impl<'ast> Visit<'ast> for Names {
        fn visit_type_path(&mut self, path: &'ast syn::TypePath) {
            if let Some(segment) = path.path.segments.last() {
                self.0.insert(segment.ident.to_string());
            }
            syn::visit::visit_type_path(self, path);
        }
    }
    let mut names = Names(BTreeSet::new());
    names.visit_return_type(output);
    names.0
}

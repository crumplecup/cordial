//! Binding and lifetime evidence for constructor initializer recognition.

use std::collections::{BTreeMap, BTreeSet};

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{
    Expr, FnArg, GenericArgument, GenericParam, Item, ItemImpl, Pat, PathArguments, Signature,
};
use tracing::instrument;

use super::replacements::is_location_caller;
use crate::etiquettes::internal_error_chain::apply::{is_static_str, span_range};

#[instrument(level = "debug", skip(signature, bindings, target_lifetime))]
pub(super) fn static_string_params(
    signature: &Signature,
    bindings: &BindingCounts,
    target_lifetime: Option<&str>,
    lifetime_matches: bool,
) -> BTreeSet<String> {
    let mut params: BTreeSet<String> = signature
        .inputs
        .iter()
        .filter_map(|arg| match arg {
            FnArg::Typed(arg)
                if is_static_str(&arg.ty)
                    || (lifetime_matches && target_lifetime.is_some_and(|lifetime| {
                        crate::etiquettes::internal_error_chain::apply::is_str_ref_with_lifetime(
                            &arg.ty, lifetime,
                        )
                    })) =>
            {
                match &*arg.pat {
                    Pat::Ident(ident) if ident.subpat.is_none() => Some(ident.ident.to_string()),
                    _ => None,
                }
            }
            _ => None,
        })
        .collect();
    params.retain(|name| !bindings.opaque_local_names && !bindings.names.contains_key(name));
    params
}

#[instrument(
    level = "debug",
    skip(signature, block, bindings, source, target_lifetime)
)]
pub(super) fn static_string_locals(
    signature: &Signature,
    block: &syn::Block,
    bindings: &BindingCounts,
    source: &str,
    target_lifetime: Option<&str>,
    lifetime_matches: bool,
) -> BTreeMap<String, usize> {
    if bindings.opaque_local_names {
        return BTreeMap::new();
    }
    let argument_names: BTreeSet<String> = signature
        .inputs
        .iter()
        .filter_map(|arg| match arg {
            FnArg::Typed(arg) => match &*arg.pat {
                Pat::Ident(ident) => Some(ident.ident.to_string()),
                _ => None,
            },
            FnArg::Receiver(_) => None,
        })
        .collect();
    block
        .stmts
        .iter()
        .filter_map(|statement| {
            let (name, declaration_end) = match statement {
                syn::Stmt::Local(local) => {
                    let Pat::Type(typed) = &local.pat else {
                        return None;
                    };
                    let Pat::Ident(ident) = &*typed.pat else {
                        return None;
                    };
                    if !local.attrs.is_empty()
                        || local.init.is_none()
                        || ident.mutability.is_some()
                        || ident.subpat.is_some()
                        || !(is_static_str(&typed.ty)
                            || (lifetime_matches
                                && target_lifetime.is_some_and(|lifetime| {
                                    crate::etiquettes::internal_error_chain::apply::is_str_ref_with_lifetime(&typed.ty, lifetime)
                                })))
                    {
                        return None;
                    }
                    let (_, end) = span_range(source, local.span())?;
                    (ident.ident.to_string(), end)
                }
                syn::Stmt::Item(Item::Const(item))
                    if item.attrs.is_empty() && is_static_str(&item.ty) =>
                {
                    (item.ident.to_string(), 0)
                }
                syn::Stmt::Item(Item::Static(item))
                    if item.attrs.is_empty()
                        && matches!(item.mutability, syn::StaticMutability::None)
                        && is_static_str(&item.ty) =>
                {
                    (item.ident.to_string(), 0)
                }
                _ => return None,
            };
            (!argument_names.contains(&name) && bindings.names.get(&name) == Some(&1))
                .then_some((name, declaration_end))
        })
        .collect()
}

#[instrument(level = "trace", skip(item, type_name, lifetime))]
pub(crate) fn impl_uses_lifetime(item: &ItemImpl, type_name: &str, lifetime: &str) -> bool {
    if item.generics.where_clause.is_some() || item.generics.params.len() != 1 {
        return false;
    }
    let Some(GenericParam::Lifetime(param)) = item.generics.params.first() else {
        return false;
    };
    if param.lifetime.ident != lifetime || !param.bounds.is_empty() || !param.attrs.is_empty() {
        return false;
    }
    let syn::Type::Path(path) = &*item.self_ty else {
        return false;
    };
    if path.qself.is_some() || path.path.segments.len() != 1 {
        return false;
    }
    let Some(segment) = path.path.segments.first() else {
        return false;
    };
    if segment.ident != type_name {
        return false;
    }
    matches!(&segment.arguments, PathArguments::AngleBracketed(args)
        if matches!(args.args.first(), Some(GenericArgument::Lifetime(named)) if named.ident == lifetime)
            && args.args.len() == 1)
}

#[instrument(level = "debug", skip(block, bindings))]
pub(super) fn caller_aliases(block: &syn::Block, bindings: &BindingCounts) -> BTreeSet<String> {
    let mut aliases: BTreeSet<String> = block.stmts.iter().filter_map(|stmt| {
        let syn::Stmt::Local(local) = stmt else { return None };
        let Pat::Ident(pat) = &local.pat else { return None };
        if pat.mutability.is_some() || !matches!(local.init.as_ref().map(|init| &*init.expr), Some(Expr::Call(call)) if is_location_caller(call)) {
            return None;
        }
        Some(pat.ident.to_string())
    }).collect();
    aliases.retain(|name| !bindings.opaque_local_names && bindings.names.get(name) == Some(&1));
    aliases
}

#[derive(Default)]
pub(super) struct BindingCounts {
    names: BTreeMap<String, usize>,
    opaque_local_names: bool,
}

impl<'ast> Visit<'ast> for BindingCounts {
    #[instrument(level = "debug", skip(self, pat))]
    fn visit_pat_ident(&mut self, pat: &'ast syn::PatIdent) {
        *self.names.entry(pat.ident.to_string()).or_default() += 1;
        syn::visit::visit_pat_ident(self, pat);
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_item(&mut self, item: &'ast Item) {
        let name = match item {
            Item::Const(item) => Some(&item.ident),
            Item::Static(item) => Some(&item.ident),
            Item::Fn(item) => Some(&item.sig.ident),
            Item::Struct(item) => Some(&item.ident),
            Item::Enum(item) => Some(&item.ident),
            Item::Union(item) => Some(&item.ident),
            Item::Use(_) | Item::Macro(_) => {
                self.opaque_local_names = true;
                None
            }
            _ => None,
        };
        if let Some(name) = name {
            *self.names.entry(name.to_string()).or_default() += 1;
        }
        syn::visit::visit_item(self, item);
    }
}

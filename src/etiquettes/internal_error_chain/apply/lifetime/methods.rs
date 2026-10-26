//! Constructor and impl edits for lifetime migration.

use syn::spanned::Spanned;
use syn::{FnArg, GenericParam, ImplItem, ItemImpl, ItemStruct, Pat, ReturnType, Type};
use tracing::instrument;

use crate::config::ErrorFieldStrategy;
use crate::loader::SharedParsedFile;

use super::super::{is_str_ref_with_lifetime, span_range};
use super::{ImplPlanFacts, LifetimeFileIndex, scalar_copy_name};

#[instrument(level = "debug", skip(index))]
pub(super) fn constructor_lifetime(
    index: &LifetimeFileIndex<'_>,
    type_name: &str,
) -> Option<String> {
    let mut found = None;
    for &implementation in index.impls.get(type_name).map(Vec::as_slice).unwrap_or(&[]) {
        if implementation.trait_.is_some() {
            continue;
        }
        let [ImplItem::Fn(method)] = implementation.items.as_slice() else {
            return None;
        };
        let Some(GenericParam::Lifetime(param)) = method.sig.generics.params.first() else {
            return None;
        };
        if method.sig.generics.params.len() != 1 {
            return None;
        }
        if found.replace(param.lifetime.ident.to_string()).is_some() {
            return None;
        }
    }
    found
}

#[instrument(level = "debug", skip(method, item, strategy))]
pub(super) fn constructor_signature(
    method: &syn::ImplItemFn,
    item: &ItemStruct,
    field_name: &str,
    lifetime: &str,
    strategy: ErrorFieldStrategy,
) -> bool {
    if !method.attrs.is_empty()
        || method.sig.asyncness.is_some()
        || matches!(method.sig.safety, syn::Safety::Unsafe(_))
        || method.sig.constness.is_some()
        || method.sig.generics.where_clause.is_some()
        || method.sig.inputs.is_empty()
        || !matches!(&method.sig.output, ReturnType::Type(_, ty) if matches!(&**ty, Type::Path(path) if path.path.is_ident("Self")))
    {
        return false;
    }
    let valid_generics = match strategy {
        ErrorFieldStrategy::Borrowed => {
            matches!(method.sig.generics.params.first(),
            Some(GenericParam::Lifetime(param)) if param.lifetime.ident == lifetime && param.bounds.is_empty() && param.attrs.is_empty())
                && method.sig.generics.params.len() == 1
        }
        ErrorFieldStrategy::Owned => method.sig.generics.params.is_empty(),
    };
    let syn::Fields::Named(fields) = &item.fields else {
        return false;
    };
    valid_generics
        && matches!(method.sig.inputs.first(), Some(FnArg::Typed(arg))
        if matches!(&*arg.pat, Pat::Ident(pat) if pat.ident == field_name && pat.subpat.is_none())
            && is_str_ref_with_lifetime(&arg.ty, lifetime))
        && method.sig.inputs.iter().skip(1).all(|input| {
            let FnArg::Typed(argument) = input else {
                return false;
            };
            let Pat::Ident(pattern) = &*argument.pat else {
                return false;
            };
            if pattern.subpat.is_some() || pattern.by_ref.is_some() || pattern.mutability.is_some()
            {
                return false;
            }
            fields.named.iter().any(|field| {
                field.ident.as_ref() == Some(&pattern.ident)
                    && pattern.ident != field_name
                    && scalar_copy_name(&field.ty)
                        .is_some_and(|name| scalar_copy_name(&argument.ty) == Some(name))
            })
        })
}

#[instrument(level = "debug", skip(file, implementation, strategy))]
pub(super) fn impl_facts(
    file: &SharedParsedFile,
    implementation: &ItemImpl,
    strategy: ErrorFieldStrategy,
) -> Option<ImplPlanFacts> {
    let self_type = span_range(file.source(), implementation.self_ty.span())?;
    let impl_token_end = (strategy == ErrorFieldStrategy::Borrowed)
        .then(|| span_range(file.source(), implementation.impl_token.span()).map(|(_, end)| end))
        .flatten();
    let generics = (strategy == ErrorFieldStrategy::Owned)
        .then(|| span_range(file.source(), implementation.generics.span()))
        .flatten();
    let (constructor_generics, constructor_ident_end) = if implementation.trait_.is_none() {
        let Some(ImplItem::Fn(method)) = implementation.items.first() else {
            return None;
        };
        match strategy {
            ErrorFieldStrategy::Borrowed => (
                Some(span_range(file.source(), method.sig.generics.span())?),
                None,
            ),
            ErrorFieldStrategy::Owned => (
                None,
                Some(span_range(file.source(), method.sig.ident.span())?.1),
            ),
        }
    } else {
        (None, None)
    };
    Some(ImplPlanFacts {
        self_type,
        impl_token_end,
        generics,
        constructor_generics,
        constructor_ident_end,
    })
}

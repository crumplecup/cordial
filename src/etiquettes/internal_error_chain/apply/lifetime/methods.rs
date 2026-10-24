//! Constructor and impl edits for lifetime migration.

use syn::spanned::Spanned;
use syn::{FnArg, GenericParam, ImplItem, ItemImpl, ItemStruct, Pat, ReturnType, Type};
use tracing::instrument;

use crate::config::ErrorFieldStrategy;
use crate::loader::SharedParsedFile;

use super::super::{Edit, is_str_ref_with_lifetime, simple_type_name, span_range};
use super::{LifetimeFileIndex, scalar_copy_name};

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

#[instrument(level = "debug", skip(file, implementation, strategy, edits))]
pub(super) fn edit_impl(
    file: &SharedParsedFile,
    implementation: &ItemImpl,
    lifetime: &str,
    strategy: ErrorFieldStrategy,
    edits: &mut Vec<Edit>,
) -> Option<()> {
    let (start, end) = span_range(file.source(), implementation.self_ty.span())?;
    let name = simple_type_name(&implementation.self_ty)?;
    edits.push(Edit {
        start,
        end,
        replacement: match strategy {
            ErrorFieldStrategy::Borrowed => format!("{name}<'{lifetime}>"),
            ErrorFieldStrategy::Owned => name,
        },
    });
    match strategy {
        ErrorFieldStrategy::Borrowed => {
            let (_, insert) = span_range(file.source(), implementation.impl_token.span())?;
            edits.push(Edit {
                start: insert,
                end: insert,
                replacement: format!("<'{lifetime}>"),
            });
            if implementation.trait_.is_none() {
                let Some(ImplItem::Fn(method)) = implementation.items.first() else {
                    return None;
                };
                let (start, end) = span_range(file.source(), method.sig.generics.span())?;
                edits.push(Edit {
                    start,
                    end,
                    replacement: String::new(),
                });
            }
        }
        ErrorFieldStrategy::Owned => {
            let (start, end) = span_range(file.source(), implementation.generics.span())?;
            edits.push(Edit {
                start,
                end,
                replacement: String::new(),
            });
            if implementation.trait_.is_none() {
                let Some(ImplItem::Fn(method)) = implementation.items.first() else {
                    return None;
                };
                let (_, insert) = span_range(file.source(), method.sig.ident.span())?;
                edits.push(Edit {
                    start: insert,
                    end: insert,
                    replacement: format!("<'{lifetime}>"),
                });
            }
        }
    }
    Some(())
}

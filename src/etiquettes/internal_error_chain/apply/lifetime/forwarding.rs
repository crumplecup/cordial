//! Return-type edits for a direct constructor forwarding function.

use syn::spanned::Spanned;
use syn::{Expr, FnArg, GenericParam, ItemFn, Pat, ReturnType, Type};
use tracing::instrument;

use crate::config::ErrorFieldStrategy;
use crate::loader::SharedParsedFile;

use super::super::{Edit, is_str_ref_with_lifetime, span_range};
use super::scalar_copy_name;

#[instrument(level = "debug", skip(file, function, strategy))]
pub(super) fn forwarding_return(
    file: &SharedParsedFile,
    function: &ItemFn,
    type_name: &str,
    field_name: &str,
    constructor_name: &str,
    lifetime: &str,
    strategy: ErrorFieldStrategy,
) -> Option<((usize, usize), Edit)> {
    if !function.attrs.is_empty()
        || !matches!(function.vis, syn::Visibility::Inherited)
        || function.sig.asyncness.is_some()
        || function.sig.constness.is_some()
        || matches!(function.sig.safety, syn::Safety::Unsafe(_))
        || function.sig.generics.where_clause.is_some()
        || function.sig.generics.params.len() != 1
        || function.sig.inputs.is_empty()
        || function.block.stmts.len() != 1
    {
        return None;
    }
    let Some(GenericParam::Lifetime(param)) = function.sig.generics.params.first() else {
        return None;
    };
    if param.lifetime.ident != lifetime || !param.attrs.is_empty() || !param.bounds.is_empty() {
        return None;
    }
    let Some(FnArg::Typed(argument)) = function.sig.inputs.first() else {
        return None;
    };
    if !matches!(&*argument.pat, Pat::Ident(pat) if pat.ident == field_name && pat.subpat.is_none())
        || !is_str_ref_with_lifetime(&argument.ty, lifetime)
    {
        return None;
    }
    let ReturnType::Type(_, return_ty) = &function.sig.output else {
        return None;
    };
    let Type::Path(return_path) = &**return_ty else {
        return None;
    };
    if return_path.qself.is_some() || return_path.path.segments.len() != 1 {
        return None;
    }
    let segment = return_path.path.segments.first()?;
    if segment.ident != type_name {
        return None;
    }
    let correct_arguments = match strategy {
        ErrorFieldStrategy::Borrowed => matches!(segment.arguments, syn::PathArguments::None),
        ErrorFieldStrategy::Owned => {
            matches!(&segment.arguments, syn::PathArguments::AngleBracketed(args)
            if matches!(args.args.first(), Some(syn::GenericArgument::Lifetime(named)) if named.ident == lifetime)
                && args.args.len() == 1)
        }
    };
    if !correct_arguments {
        return None;
    }
    let [syn::Stmt::Expr(Expr::Call(call), None)] = function.block.stmts.as_slice() else {
        return None;
    };
    let Expr::Path(callee) = &*call.func else {
        return None;
    };
    let segments: Vec<_> = callee.path.segments.iter().collect();
    let [type_part, method_part] = segments.as_slice() else {
        return None;
    };
    if callee.qself.is_some()
        || type_part.ident != type_name
        || !matches!(type_part.arguments, syn::PathArguments::None)
        || method_part.ident != constructor_name
        || !matches!(method_part.arguments, syn::PathArguments::None)
        || call.args.len() != function.sig.inputs.len()
        || !matches!(call.args.first(), Some(Expr::Path(path)) if path.path.is_ident(field_name))
    {
        return None;
    }
    if !function
        .sig
        .inputs
        .iter()
        .skip(1)
        .zip(call.args.iter().skip(1))
        .all(|(input, passed)| {
            matches!((input, passed), (FnArg::Typed(argument), Expr::Path(path))
                if matches!(&*argument.pat, Pat::Ident(pattern)
                    if pattern.subpat.is_none()
                        && pattern.by_ref.is_none()
                        && pattern.mutability.is_none()
                        && path.path.is_ident(&pattern.ident))
                    && scalar_copy_name(&argument.ty).is_some())
        })
    {
        return None;
    }
    let start = return_ty.span().start();
    let (edit_start, edit_end) = span_range(file.source(), return_ty.span())?;
    Some((
        (start.line, start.column),
        Edit {
            start: edit_start,
            end: edit_end,
            replacement: match strategy {
                ErrorFieldStrategy::Borrowed => format!("{type_name}<'{lifetime}>"),
                ErrorFieldStrategy::Owned => type_name.to_string(),
            },
        },
    ))
}

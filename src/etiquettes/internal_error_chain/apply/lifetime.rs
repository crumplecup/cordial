//! A closed single-field lifetime migration in either direction.

use std::collections::BTreeSet;

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{
    Expr, FnArg, GenericParam, ImplItem, Item, ItemFn, ItemImpl, ItemStruct, Pat, ReturnType, Type,
};
use tracing::instrument;

use crate::config::ErrorFieldStrategy;
use crate::loader::SharedParsedFile;

use super::initializers::{FieldKind, InitializerVisitor};
use super::inventory::{ErrorConversionSiteKind, ErrorConversionUnit};
use super::{Edit, is_str_ref_with_lifetime, is_string, simple_type_name, span_range};

/// Collect a special lifetime migration from the shared parsed tree.
#[instrument(level = "debug", skip(file, unit, strategy))]
pub(super) fn plan_for_unit(
    file: &SharedParsedFile,
    unit: &ErrorConversionUnit,
    strategy: ErrorFieldStrategy,
) -> Option<Vec<Edit>> {
    let item = file.syntax().items.iter().find_map(|item| match item {
        Item::Struct(item) if item.ident == unit.type_name().as_str() => Some(item),
        _ => None,
    })?;
    plan(file, item, unit.field_name(), unit, strategy)
}

#[instrument(level = "trace", skip(item, field, kind, strategy))]
pub(super) fn is_candidate(
    item: &ItemStruct,
    field: &syn::Field,
    kind: FieldKind,
    struct_lifetime: Option<&str>,
    strategy: ErrorFieldStrategy,
) -> bool {
    matches!(kind, FieldKind::StaticString)
        && struct_lifetime.is_none()
        && match strategy {
            ErrorFieldStrategy::Borrowed => is_string(&field.ty) && item.generics.params.is_empty(),
            ErrorFieldStrategy::Owned => matches!(&field.ty, Type::Reference(reference)
                if reference.lifetime.as_ref().is_some_and(|named| named.ident != "static")
                    && matches!(&*reference.elem, Type::Path(path) if path.path.is_ident("str"))),
        }
}

/// Move the one constructor lifetime onto the private error type, or undo it.
///
/// This deliberately accepts only a single-field, single-constructor unit.
/// Type mentions must be impl self types or direct forwarding function returns;
/// inference-based direct calls are safe because their signatures remain unchanged.
#[instrument(level = "debug", skip(file, item, unit, strategy))]
pub(super) fn plan(
    file: &SharedParsedFile,
    item: &ItemStruct,
    field_name: &str,
    unit: &ErrorConversionUnit,
    strategy: ErrorFieldStrategy,
) -> Option<Vec<Edit>> {
    let syn::Fields::Named(fields) = &item.fields else {
        return None;
    };
    if fields.named.len() != 1 || !unit.is_closed() || unit.blocks_existing_field_edit() {
        return None;
    }
    let field = fields.named.first()?;
    let lifetime = match strategy {
        ErrorFieldStrategy::Borrowed if is_string(&field.ty) && item.generics.params.is_empty() => {
            constructor_lifetime(file, &item.ident.to_string())?
        }
        ErrorFieldStrategy::Owned if item.generics.params.len() == 1 => {
            let GenericParam::Lifetime(param) = item.generics.params.first()? else {
                return None;
            };
            let lifetime = param.lifetime.ident.to_string();
            if !param.attrs.is_empty()
                || !param.bounds.is_empty()
                || !is_str_ref_with_lifetime(&field.ty, &lifetime)
            {
                return None;
            }
            lifetime
        }
        _ => return None,
    };
    if item.generics.where_clause.is_some()
        || !item.attrs.iter().all(|attr| attr.path().is_ident("derive"))
    {
        return None;
    }

    let mut impls = Vec::new();
    let mut allowed_type_uses = BTreeSet::new();
    let mut constructors = 0;
    let mut constructor_name = None;
    for syntax_item in &file.syntax().items {
        let Item::Impl(implementation) = syntax_item else {
            continue;
        };
        if simple_type_name(&implementation.self_ty).as_deref() != Some(&item.ident.to_string()) {
            continue;
        }
        if !implementation.attrs.is_empty()
            || implementation.unsafety.is_some()
            || implementation.modifiers.defaultness.is_some()
            || implementation.generics.where_clause.is_some()
        {
            return None;
        }
        let valid_impl = match strategy {
            ErrorFieldStrategy::Borrowed => implementation.generics.params.is_empty(),
            ErrorFieldStrategy::Owned => super::initializers::impl_uses_lifetime(
                implementation,
                &item.ident.to_string(),
                &lifetime,
            ),
        };
        if !valid_impl {
            return None;
        }
        let start = implementation.self_ty.span().start();
        allowed_type_uses.insert((start.line, start.column));
        if let Some((trait_path, _)) = &implementation.trait_ {
            let segments: Vec<_> = trait_path.segments.iter().collect();
            if !matches!(segments.as_slice(), [root, module, name]
                if matches!(root.ident.to_string().as_str(), "std" | "core")
                    && matches!((module.ident.to_string().as_str(), name.ident.to_string().as_str()),
                        ("fmt", "Display") | ("error", "Error"))
                    && segments.iter().all(|segment| matches!(segment.arguments, syn::PathArguments::None)))
            {
                return None;
            }
        } else {
            if implementation.items.len() != 1 {
                return None;
            }
            let Some(ImplItem::Fn(method)) = implementation.items.first() else {
                return None;
            };
            if !constructor_signature(method, field_name, &lifetime, strategy) {
                return None;
            }
            constructors += 1;
            constructor_name = Some(method.sig.ident.to_string());
        }
        impls.push(implementation);
    }
    if constructors != 1 {
        return None;
    }
    let mut return_edits = Vec::new();
    for syntax_item in &file.syntax().items {
        let Item::Fn(function) = syntax_item else {
            continue;
        };
        if let Some((position, edit)) = forwarding_return(
            file,
            function,
            &item.ident.to_string(),
            field_name,
            constructor_name.as_deref()?,
            &lifetime,
            strategy,
        ) {
            allowed_type_uses.insert(position);
            return_edits.push(edit);
        }
    }
    if unit.sites().iter().any(|site| {
        site.kind() == ErrorConversionSiteKind::TypeUse
            && !allowed_type_uses.contains(&(site.line(), site.column()))
    }) {
        return None;
    }
    let empty = BTreeSet::new();
    let type_name = item.ident.to_string();
    let mut initializer = InitializerVisitor::new(
        &type_name,
        field_name,
        FieldKind::StaticString,
        strategy,
        file.source(),
        &empty,
        Some(&lifetime),
        strategy == ErrorFieldStrategy::Borrowed,
    );
    initializer.visit_file(file.syntax());
    let mut edits = initializer.finish()?;
    edits.append(&mut return_edits);
    let (field_start, field_end) = span_range(file.source(), field.ty.span())?;
    edits.push(Edit {
        start: field_start,
        end: field_end,
        replacement: match strategy {
            ErrorFieldStrategy::Borrowed => format!("&'{lifetime} str"),
            ErrorFieldStrategy::Owned => "String".to_string(),
        },
    });
    match strategy {
        ErrorFieldStrategy::Borrowed => {
            let (_, insert) = span_range(file.source(), item.ident.span())?;
            edits.push(Edit {
                start: insert,
                end: insert,
                replacement: format!("<'{lifetime}>"),
            });
        }
        ErrorFieldStrategy::Owned => {
            let (start, end) = span_range(file.source(), item.generics.span())?;
            edits.push(Edit {
                start,
                end,
                replacement: String::new(),
            });
        }
    }
    for implementation in impls {
        edit_impl(file, implementation, &lifetime, strategy, &mut edits)?;
    }
    Some(edits)
}

#[instrument(level = "debug", skip(file, function, strategy))]
fn forwarding_return(
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
        || function.sig.inputs.len() != 1
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
        || call.args.len() != 1
        || !matches!(call.args.first(), Some(Expr::Path(path)) if path.path.is_ident(field_name))
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

#[instrument(level = "debug", skip(file))]
fn constructor_lifetime(file: &SharedParsedFile, type_name: &str) -> Option<String> {
    let mut found = None;
    for item in &file.syntax().items {
        let Item::Impl(implementation) = item else {
            continue;
        };
        if implementation.trait_.is_some()
            || simple_type_name(&implementation.self_ty).as_deref() != Some(type_name)
        {
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

#[instrument(level = "debug", skip(method, strategy))]
fn constructor_signature(
    method: &syn::ImplItemFn,
    field_name: &str,
    lifetime: &str,
    strategy: ErrorFieldStrategy,
) -> bool {
    if !method.attrs.is_empty()
        || method.sig.asyncness.is_some()
        || matches!(method.sig.safety, syn::Safety::Unsafe(_))
        || method.sig.constness.is_some()
        || method.sig.generics.where_clause.is_some()
        || method.sig.inputs.len() != 1
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
    valid_generics
        && matches!(method.sig.inputs.first(), Some(FnArg::Typed(arg))
        if matches!(&*arg.pat, Pat::Ident(pat) if pat.ident == field_name && pat.subpat.is_none())
            && is_str_ref_with_lifetime(&arg.ty, lifetime))
}

#[instrument(level = "debug", skip(file, implementation, strategy, edits))]
fn edit_impl(
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

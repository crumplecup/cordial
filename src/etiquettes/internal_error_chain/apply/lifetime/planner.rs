//! Closed-unit planning for lifetime migration.

use std::collections::BTreeSet;

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{GenericParam, ImplItem, ItemStruct};
use tracing::instrument;

use crate::config::ErrorFieldStrategy;
use crate::loader::SharedParsedFile;

use super::super::initializers::{FieldKind, InitializerVisitor, impl_uses_lifetime};
use super::super::inventory::{ErrorConversionSiteKind, ErrorConversionUnit};
use super::super::{Edit, is_str_ref_with_lifetime, is_string, span_range};
use super::forwarding::forwarding_return;
use super::methods::{constructor_lifetime, constructor_signature, edit_impl};
use super::{LifetimeFileIndex, scalar_copy_name};

/// Move the one constructor lifetime onto the private error type, or undo it.
///
/// This accepts one migrated field, optional scalar context, and one constructor.
/// Type mentions must be impl self types or direct forwarding function returns;
/// inference-based direct calls are safe because their signatures remain unchanged.
#[instrument(level = "debug", skip(file, index, item, unit, strategy))]
pub(super) fn plan(
    file: &SharedParsedFile,
    index: &LifetimeFileIndex<'_>,
    item: &ItemStruct,
    field_name: &str,
    unit: &ErrorConversionUnit,
    strategy: ErrorFieldStrategy,
) -> Option<Vec<Edit>> {
    let syn::Fields::Named(fields) = &item.fields else {
        return None;
    };
    if !unit.is_closed() || unit.blocks_existing_field_edit() {
        return None;
    }
    let field = fields
        .named
        .iter()
        .find(|field| field.ident.as_ref().is_some_and(|name| name == field_name))?;
    if fields
        .named
        .iter()
        .any(|other| other.ident != field.ident && scalar_copy_name(&other.ty).is_none())
    {
        return None;
    }
    let lifetime = match strategy {
        ErrorFieldStrategy::Borrowed if is_string(&field.ty) && item.generics.params.is_empty() => {
            constructor_lifetime(index, &item.ident.to_string())?
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
    for &implementation in index
        .impls
        .get(&item.ident.to_string())
        .map(Vec::as_slice)
        .unwrap_or(&[])
    {
        if !implementation.attrs.is_empty()
            || implementation.unsafety.is_some()
            || implementation.modifiers.defaultness.is_some()
            || implementation.generics.where_clause.is_some()
        {
            return None;
        }
        let valid_impl = match strategy {
            ErrorFieldStrategy::Borrowed => implementation.generics.params.is_empty(),
            ErrorFieldStrategy::Owned => {
                impl_uses_lifetime(implementation, &item.ident.to_string(), &lifetime)
            }
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
            if !constructor_signature(method, item, field_name, &lifetime, strategy) {
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
    for &function in &index.functions {
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

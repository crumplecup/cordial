//! Extract graph declarations while the shared parsed file is in collection.

use syn::spanned::Spanned;
use syn::{Item, Type};
use tracing::instrument;

use crate::loader::SharedParsedFile;

use super::super::super::{is_string, known_error_impls, single_struct_lifetime, span_range};
use super::super::types::{HasGenerics, allowed_attrs, is_copy_shape, storage_shape, type_name};
use super::{
    DeclarationKind, GraphDeclaration, GraphDeclarationFacts, GraphField, GraphFields,
    GraphVariant, Visibility,
};
use crate::etiquettes::internal_error_chain::apply::inventory::{
    ErrorEdgeField, StringStorage, error_edge_evidence,
};
use crate::etiquettes::internal_error_chain::type_graph::item_derives_error;

impl GraphDeclarationFacts {
    #[instrument(level = "debug", skip(file))]
    pub(in crate::etiquettes::internal_error_chain::apply) fn collect(
        file: &SharedParsedFile,
    ) -> Self {
        let error_impls = known_error_impls(file.syntax());
        let declarations = file
            .syntax()
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Struct(item) => {
                    let location = item.ident.span().start();
                    Some(GraphDeclaration {
                        name: item.ident.to_string(),
                        line: location.line,
                        declaration_line: item.span().start().line,
                        column: location.column,
                        kind: DeclarationKind::Struct,
                        visibility: visibility(&item.vis),
                        allowed_attrs: allowed_attrs(&item.attrs),
                        error_impl: error_impls.contains(&item.ident.to_string()),
                        derives_error: item_derives_error(&item.attrs),
                        generics_empty: item.generics.params.is_empty(),
                        single_struct_lifetime: single_struct_lifetime(item),
                        only_lifetime: item.only_lifetime(),
                        fields: fields(file, &item.fields),
                        variants: Vec::new(),
                    })
                }
                Item::Enum(item) => {
                    let location = item.ident.span().start();
                    Some(GraphDeclaration {
                        name: item.ident.to_string(),
                        line: location.line,
                        declaration_line: item.span().start().line,
                        column: location.column,
                        kind: DeclarationKind::Enum,
                        visibility: visibility(&item.vis),
                        allowed_attrs: allowed_attrs(&item.attrs),
                        error_impl: false,
                        derives_error: false,
                        generics_empty: item.generics.params.is_empty(),
                        single_struct_lifetime: None,
                        only_lifetime: item.only_lifetime(),
                        fields: GraphFields::Unit,
                        variants: item
                            .variants
                            .iter()
                            .map(|variant| GraphVariant {
                                name: variant.ident.to_string(),
                                allowed_attrs: allowed_attrs(&variant.attrs),
                                fields: fields(file, &variant.fields),
                            })
                            .collect(),
                    })
                }
                _ => None,
            })
            .collect();
        Self { declarations }
    }
}

#[instrument(level = "trace", skip(value))]
fn visibility(value: &syn::Visibility) -> Visibility {
    match value {
        syn::Visibility::Inherited => Visibility::Inherited,
        syn::Visibility::Public(_) => Visibility::Public,
        syn::Visibility::Restricted(_) => Visibility::Restricted,
    }
}

#[instrument(level = "trace", skip(file, value))]
fn fields(file: &SharedParsedFile, value: &syn::Fields) -> GraphFields {
    match value {
        syn::Fields::Named(fields) => GraphFields::Named(
            fields
                .named
                .iter()
                .map(|field| graph_field(file, field))
                .collect(),
        ),
        syn::Fields::Unnamed(fields) => GraphFields::Unnamed(
            fields
                .unnamed
                .iter()
                .map(|field| graph_field(file, field))
                .collect(),
        ),
        syn::Fields::Unit => GraphFields::Unit,
    }
}

#[instrument(level = "trace", skip(file, field))]
fn graph_field(file: &SharedParsedFile, field: &syn::Field) -> GraphField {
    let type_range = span_range(file.source(), field.ty.span());
    let type_text = type_range.map(|(start, end)| file.source()[start..end].to_string());
    let borrowed_referent = match &field.ty {
        Type::Path(path) if path.path.is_ident("String") => Some("str".to_string()),
        Type::Path(path)
            if path.path.segments.len() == 1 && path.path.segments[0].ident == "Box" =>
        {
            match &path.path.segments[0].arguments {
                syn::PathArguments::AngleBracketed(arguments) => {
                    arguments.args.first().and_then(|arg| {
                        let syn::GenericArgument::Type(inner) = arg else {
                            return None;
                        };
                        span_range(file.source(), inner.span())
                            .map(|(start, end)| file.source()[start..end].to_string())
                    })
                }
                _ => None,
            }
        }
        _ => type_text.clone(),
    };
    let owned_referent = match &field.ty {
        Type::Reference(reference) => span_range(file.source(), reference.elem.span())
            .map(|(start, end)| file.source()[start..end].to_string()),
        _ => None,
    };
    let stripped_lifetime = stripped_lifetime(file, &field.ty);
    GraphField {
        name: field.ident.as_ref().map(ToString::to_string),
        line: field.span().start().line,
        type_line: field.ty.span().start().line,
        visibility: visibility(&field.vis),
        allowed_attrs: allowed_attrs(&field.attrs),
        type_name: type_name(&field.ty),
        storage: storage_shape(&field.ty),
        copy_shape: is_copy_shape(&field.ty),
        reference: matches!(field.ty, Type::Reference(_)),
        unit_reference: matches!(&field.ty, Type::Reference(reference)
            if matches!(&*reference.elem, Type::Tuple(tuple) if tuple.elems.is_empty())),
        type_range,
        borrowed_referent,
        owned_referent,
        stripped_lifetime,
        source_edge: (field.ident.is_none()
            || field.ident.as_ref().is_some_and(|name| name == "source"))
        .then(|| error_edge_evidence(file, field, ErrorEdgeField::Source))
        .flatten(),
        kind_edge: (field.ident.as_ref().is_some_and(|name| name == "kind"))
            .then(|| error_edge_evidence(file, field, ErrorEdgeField::Kind))
            .flatten(),
        candidate_storage: if is_string(&field.ty) {
            Some(StringStorage::Owned)
        } else if let Type::Reference(reference) = &field.ty {
            matches!(&*reference.elem, Type::Path(path) if path.path.is_ident("str")).then(|| {
                StringStorage::Borrowed {
                    lifetime: reference
                        .lifetime
                        .as_ref()
                        .map(|value| value.ident.to_string()),
                    mutable: reference.mutability.is_some(),
                }
            })
        } else {
            None
        },
    }
}

#[instrument(level = "trace", skip(file, ty))]
fn stripped_lifetime(file: &SharedParsedFile, ty: &Type) -> Option<(String, String)> {
    let Type::Reference(reference) = ty else {
        return None;
    };
    let Type::Path(path) = &*reference.elem else {
        return None;
    };
    let segment = path.path.segments.last()?;
    let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return None;
    };
    let [syn::GenericArgument::Lifetime(lifetime)] =
        arguments.args.iter().collect::<Vec<_>>().as_slice()
    else {
        return None;
    };
    let (args_start, args_end) = span_range(file.source(), arguments.span())?;
    let (type_start, type_end) = span_range(file.source(), reference.elem.span())?;
    if args_start < type_start || args_end > type_end {
        return None;
    }
    Some((
        lifetime.ident.to_string(),
        format!(
            "{}{}",
            &file.source()[type_start..args_start],
            &file.source()[args_end..type_end]
        ),
    ))
}

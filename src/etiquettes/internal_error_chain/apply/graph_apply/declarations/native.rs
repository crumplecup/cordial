//! Error declaration contracts from the canonical database syntax.
use super::{DeclarationKind, GraphDeclaration, GraphDeclarationFacts, GraphFields, GraphVariant};
use crate::loader::DatabaseSource;
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasGenericParams, HasModuleItem, HasName, HasVisibility},
};
use std::collections::BTreeSet;

mod fields;
mod types;

impl GraphDeclarationFacts {
    #[tracing::instrument(level = "debug", skip(file))]
    #[hotpath::measure(impl_type = "GraphDeclarationFacts")]
    pub(crate) fn collect(file: &DatabaseSource<'_>) -> Self {
        let error_impls: BTreeSet<_> = file
            .tree()
            .items()
            .filter_map(|item| {
                let ast::Item::Impl(item) = item else {
                    return None;
                };
                let ast::Type::PathType(trait_ty) = item.trait_()? else {
                    return None;
                };
                if trait_ty.path()?.segment()?.name_ref()?.text() != "Error" {
                    return None;
                }
                let ast::Type::PathType(ty) = item.self_ty()? else {
                    return None;
                };
                let path = ty.path()?;
                (path.qualifier().is_none() && path.segment()?.type_anchor().is_none())
                    .then(|| {
                        path.segment()?
                            .name_ref()
                            .map(|name| name.text().to_string())
                    })
                    .flatten()
            })
            .collect();
        let declarations = file
            .tree()
            .items()
            .filter_map(|item| {
                let (
                    kind,
                    node,
                    name,
                    visibility,
                    generics,
                    single_struct_lifetime,
                    fields,
                    variants,
                ) = match item {
                    ast::Item::Struct(item) => (
                        DeclarationKind::Struct,
                        item.syntax().clone(),
                        item.name()?,
                        item.visibility(),
                        item.generic_param_list(),
                        types::single_lifetime(&item),
                        fields::collect(file, item.field_list()),
                        Vec::new(),
                    ),
                    ast::Item::Enum(item) => (
                        DeclarationKind::Enum,
                        item.syntax().clone(),
                        item.name()?,
                        item.visibility(),
                        item.generic_param_list(),
                        None,
                        GraphFields::Unit,
                        item.variant_list()
                            .into_iter()
                            .flat_map(|list| list.variants())
                            .filter_map(|variant| {
                                Some(GraphVariant {
                                    name: variant.name()?.text().to_string(),
                                    allowed_attrs: types::allowed_attrs(&variant),
                                    fields: fields::collect(file, variant.field_list()),
                                })
                            })
                            .collect(),
                    ),
                    _ => return None,
                };
                let location = file.line(name.syntax().text_range().start()) as usize;
                let name_text = name.text().to_string();
                let owner = ast::AnyHasAttrs::cast(node.clone())?;
                Some(GraphDeclaration {
                    name: name_text.clone(),
                    line: location,
                    column: types::column(file, name.syntax()),
                    declaration_line: types::start_line(file, &node),
                    kind,
                    visibility: types::visibility(visibility),
                    allowed_attrs: types::allowed_attrs(&owner),
                    linkable_attrs: types::builtin_attrs(&owner),
                    error_impl: kind == DeclarationKind::Struct && error_impls.contains(&name_text),
                    derives_error: kind == DeclarationKind::Struct
                        && crate::ir::derives_error(&owner),
                    generics_empty: generics
                        .as_ref()
                        .is_none_or(|list| list.generic_params().next().is_none()),
                    single_struct_lifetime,
                    only_lifetime: types::only_lifetime(generics.as_ref()),
                    fields,
                    variants,
                })
            })
            .collect();
        Self { declarations }
    }
}

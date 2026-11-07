//! Derive declaration evidence from the canonical database tree.
mod attrs;
mod constructor;
mod exprs;
mod fields;
mod methods;
mod setters;

use crate::ir::{
    DeriveFileFacts, DeriveImplFacts, DeriveItemFacts, DeriveModuleFacts, DeriveStructFacts,
};
use crate::{CordialResult, loader::DatabaseSource};
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasModuleItem, HasName},
};
use std::path::Path;

#[tracing::instrument(level = "debug", skip(file, prefix), err(level = "warn"))]
pub(super) fn collect(
    file: &DatabaseSource<'_>,
    root: &Path,
    prefix: &[String],
) -> CordialResult<DeriveFileFacts> {
    Ok(DeriveFileFacts::new(
        file.path().strip_prefix(root)?.to_path_buf(),
        prefix.to_vec(),
        items(file.tree().items(), file)?,
    ))
}

#[tracing::instrument(level = "debug", skip(input, file), err(level = "warn"))]
fn items(
    input: impl Iterator<Item = ast::Item>,
    file: &DatabaseSource<'_>,
) -> CordialResult<Vec<DeriveItemFacts>> {
    let mut out = Vec::new();
    for item in input {
        match item {
            ast::Item::Struct(item) => {
                let name = name(&item)?;
                let (fields, exposed) = fields::collect(&item)?;
                let derives = ["Builder", "Getters", "AsRef", "Setters", "new"]
                    .into_iter()
                    .filter(|needle| attrs::has_derive(&item, needle))
                    .map(str::to_string)
                    .collect();
                out.push(DeriveItemFacts::Struct(
                    DeriveStructFacts::builder()
                        .name(name.clone())
                        .line(file.line(item.syntax().text_range().start()))
                        .derives(derives)
                        .fields(fields)
                        .exposed_fields(exposed)
                        .generated_builders(attrs::builder_names(&item, &name))
                        .clap_schema(
                            ["Parser", "Args", "Subcommand"]
                                .iter()
                                .any(|needle| attrs::has_derive(&item, needle)),
                        )
                        .creusot_only(attrs::gate(&item, "creusot"))
                        .build()?,
                ));
            }
            ast::Item::Impl(item) => {
                let ty = item
                    .self_ty()
                    .map(super::context::type_label)
                    .ok_or_else(|| {
                        crate::CordialError::not_found("derive impl self type".into())
                    })?;
                let methods = item
                    .assoc_item_list()
                    .into_iter()
                    .flat_map(|list| list.assoc_items())
                    .filter_map(|item| match item {
                        ast::AssocItem::Fn(method) => Some(method),
                        _ => None,
                    })
                    .map(|method| methods::collect(&method, &ty, file))
                    .collect::<CordialResult<_>>()?;
                out.push(DeriveItemFacts::Impl(DeriveImplFacts::new(
                    ty,
                    file.line(item.syntax().text_range().start()),
                    item.trait_().is_some(),
                    item.trait_()
                        .is_some_and(|ty| super::context::type_label(ty) == "Error"),
                    methods,
                )));
            }
            ast::Item::Module(item) => {
                if let Some(list) = item.item_list() {
                    out.push(DeriveItemFacts::Module(DeriveModuleFacts::new(
                        name(&item)?,
                        attrs::gate(&item, "test"),
                        attrs::gate(&item, "creusot"),
                        items(list.items(), file)?,
                    )));
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

#[tracing::instrument(level = "trace", skip(owner), err(level = "warn"))]
fn name(owner: &impl HasName) -> CordialResult<String> {
    owner
        .name()
        .map(|name| name.text().to_string())
        .ok_or_else(|| crate::CordialError::not_found("derive declaration name".into()))
}

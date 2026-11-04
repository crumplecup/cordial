//! Loader-owned derive evidence collection from the shared syntax carrier.
mod fields;
mod syntax;

use crate::ir::{
    DeriveFileFacts, DeriveImplFacts, DeriveItemFacts, DeriveMethodFacts, DeriveMethodFlags,
    DeriveModuleFacts, DeriveStructFacts,
};
use crate::{
    CordialResult,
    loader::{SharedParsedFile, module_path_from_src_file},
};
use std::path::Path;
use syn::spanned::Spanned;
use syntax::{
    body_is_struct_literal, classify_field_read, classify_setter_body, constructor_arg_count,
    constructor_fields_match_params, consumes_self, derive_builder_names, error_impl_target,
    has_derive, has_track_caller, is_cfg_creusot, is_cfg_test, is_clap_schema, is_fluent_setter,
    receiver_is_immutable_reference, receiver_is_mutable, type_label,
};

#[tracing::instrument(level = "debug", skip(file), err(level = "warn"))]
pub(crate) fn collect(
    file: &SharedParsedFile,
    tree_root: &Path,
    root: &Path,
) -> CordialResult<DeriveFileFacts> {
    collect_syntax(file.syntax(), file.path(), tree_root, root)
}

/// Standalone source adapter using the loader's parsing boundary.
#[tracing::instrument(level = "debug", skip(source, file), err(level = "warn"))]
pub fn scan_rust_source(
    source: &str,
    file: &Path,
    tree_root: &Path,
    root: &Path,
) -> CordialResult<DeriveFileFacts> {
    let syntax = crate::loader::source_parse(source, file, "derive-inventory")?;
    collect_syntax(&syntax, file, tree_root, root)
}

#[tracing::instrument(level = "debug", skip(syntax, file), err(level = "warn"))]
fn collect_syntax(
    syntax: &syn::File,
    file: &Path,
    tree_root: &Path,
    root: &Path,
) -> CordialResult<DeriveFileFacts> {
    Ok(DeriveFileFacts::new(
        file.strip_prefix(root).unwrap_or(file).to_path_buf(),
        module_path_from_src_file(tree_root, file),
        crate::loader::source_walk(file, "derive-inventory", || items(&syntax.items)),
    ))
}

#[tracing::instrument(level = "trace", skip(input))]
fn items(input: &[syn::Item]) -> Vec<DeriveItemFacts> {
    input
        .iter()
        .filter_map(|item| match item {
            syn::Item::Struct(item) => {
                let name = item.ident.to_string();
                let (fields, exposed) = fields::collect_struct_fields(item);
                let derives = ["Builder", "Getters", "AsRef", "Setters", "new"]
                    .into_iter()
                    .filter(|name| has_derive(&item.attrs, name))
                    .map(str::to_string)
                    .collect();
                Some(DeriveItemFacts::Struct(DeriveStructFacts::new(
                    name.clone(),
                    item.span().start().line as u32,
                    derives,
                    fields,
                    exposed,
                    derive_builder_names(&item.attrs, &name),
                    is_clap_schema(&item.attrs),
                    is_cfg_creusot(&item.attrs),
                )))
            }
            syn::Item::Impl(item) => {
                let self_type = type_label(&item.self_ty);
                let methods = item
                    .items
                    .iter()
                    .filter_map(|method| match method {
                        syn::ImplItem::Fn(method) => Some(method_facts(method, &self_type)),
                        _ => None,
                    })
                    .collect();
                Some(DeriveItemFacts::Impl(DeriveImplFacts::new(
                    self_type,
                    item.span().start().line as u32,
                    item.trait_.is_some(),
                    error_impl_target(item).is_some(),
                    methods,
                )))
            }
            syn::Item::Mod(item) => item.content.as_ref().map(|(_, nested)| {
                DeriveItemFacts::Module(DeriveModuleFacts::new(
                    item.ident.to_string(),
                    is_cfg_test(&item.attrs),
                    is_cfg_creusot(&item.attrs),
                    items(nested),
                ))
            }),
            _ => None,
        })
        .collect()
}

#[tracing::instrument(level = "trace", skip(method))]
fn method_facts(method: &syn::ImplItemFn, self_type: &str) -> DeriveMethodFacts {
    let name = method.sig.ident.to_string();
    let receiver = method.sig.receiver();
    let target = name
        .strip_prefix("with_")
        .or_else(|| name.strip_prefix("set_"))
        .unwrap_or(&name);
    let setter = classify_setter_body(&method.block, target, &method.sig);
    let return_reference = match &method.sig.output {
        syn::ReturnType::Type(_, ty) => match ty.as_ref() {
            syn::Type::Reference(reference) => Some(type_label(&reference.elem)),
            _ => None,
        },
        _ => None,
    };
    let constructor = (matches!(method.sig.output, syn::ReturnType::Type(_, _))
        && body_is_struct_literal(&method.block, self_type)
        && constructor_fields_match_params(&method.sig, &method.block))
    .then(|| constructor_arg_count(&method.sig));
    DeriveMethodFacts::new(
        name,
        method.span().start().line as u32,
        DeriveMethodFlags::new(
            method.sig.constness.is_some(),
            has_track_caller(&method.attrs),
            receiver.is_some_and(receiver_is_immutable_reference),
            receiver.is_some_and(receiver_is_mutable),
            consumes_self(&method.sig),
            is_fluent_setter(&method.sig),
        ),
        classify_field_read(&method.block),
        setter,
        return_reference,
        constructor,
    )
}

//! Collect getter contracts once, before serializing conversion evidence.

use syn::{Item, Type};
use tracing::instrument;

use crate::config::ErrorFieldStrategy;
use crate::loader::SharedParsedFile;

use super::alias::{forward, marked, reverse};
use super::derives::known_derive;
use super::names::Names;
use super::{GetterFiles, GetterStorageFacts};

#[instrument(level = "debug", skip(files, uses))]
pub(in crate::etiquettes::internal_error_chain::apply) fn collect_getter_storage(
    files: &[SharedParsedFile],
    uses: &[super::super::getter_uses::GetterUseFact],
) -> GetterFiles {
    let mut names = Names::collect(files);
    let mut facts = GetterFiles::new();
    for file in files {
        let fields = facts.entry(file.path().to_path_buf()).or_default();
        for item in &file.syntax().items {
            let Item::Struct(item) = item else { continue };
            if !has_getters(file, item) {
                continue;
            }
            for field in &item.fields {
                let Some(field_name) = field.ident.as_ref().map(ToString::to_string) else {
                    continue;
                };
                // The first contract covers static call-site files only.
                if field_name != "file" || getter_skipped(field) {
                    continue;
                }
                let owner = item.ident.to_string();
                let unresolved_uses = uses.iter().any(|usage| usage.blocks(file.path(), &owner));
                let storage = if super::super::is_string(&field.ty) {
                    let alias = names.reserve(&owner, &field_name);
                    GetterStorageFacts {
                        edits: (!unresolved_uses)
                            .then(|| forward(file, item, field, &alias))
                            .flatten(),
                        alias,
                        target: ErrorFieldStrategy::Borrowed,
                    }
                } else if let Some(alias) = generated_alias(file, &field.ty, &owner, &field_name) {
                    GetterStorageFacts {
                        alias: alias.ident.to_string(),
                        edits: (!unresolved_uses)
                            .then(|| reverse(file, item, alias, field, &names))
                            .flatten(),
                        target: ErrorFieldStrategy::Owned,
                    }
                } else if super::super::is_static_str(&field.ty) {
                    // No marker owns this field's getter attributes. Do not
                    // remove or retain a user copy option while changing it
                    // to non-Copy String storage.
                    GetterStorageFacts {
                        alias: String::new(),
                        edits: None,
                        target: ErrorFieldStrategy::Owned,
                    }
                } else {
                    continue;
                };
                fields.insert((owner, field_name), storage);
            }
        }
    }
    facts
}

#[instrument(level = "trace", skip(file, item))]
fn has_getters(file: &SharedParsedFile, item: &syn::ItemStruct) -> bool {
    item.attrs
        .iter()
        .filter(|attr| attr.path().is_ident("derive"))
        .any(|attr| {
            let mut found = false;
            let valid = attr
                .parse_nested_meta(|meta| {
                    found |= known_derive(file.syntax(), &meta.path, "derive_getters", "Getters");
                    Ok(())
                })
                .is_ok();
            valid && found
        })
}

#[instrument(level = "trace", skip(field))]
fn getter_skipped(field: &syn::Field) -> bool {
    field.attrs.iter().any(|attr| {
        attr.path().is_ident("getter")
            && attr
                .parse_args::<syn::Ident>()
                .is_ok_and(|name| name == "skip")
    })
}

#[instrument(level = "trace", skip(file, ty))]
fn generated_alias<'a>(
    file: &'a SharedParsedFile,
    ty: &Type,
    owner: &str,
    field: &str,
) -> Option<&'a syn::ItemType> {
    let Type::Path(path) = ty else { return None };
    let name = path.path.get_ident()?;
    if path.qself.is_some() {
        return None;
    }
    file.syntax().items.iter().find_map(|item| match item {
        Item::Type(item)
            if item.ident == *name
                && marked(item, owner, field)
                && super::super::is_static_str(&item.ty) =>
        {
            Some(item)
        }
        _ => None,
    })
}

//! Collect getter contracts once, before serializing conversion evidence.

use syn::{Item, Type};
use tracing::instrument;

use crate::config::ErrorFieldStrategy;
use crate::loader::SharedParsedFile;

use super::alias::{forward, marked, reverse};
use super::derives::known_derive;
use super::names::Names;
use super::{GetterFiles, GetterStorageFacts};

#[instrument(level = "debug", skip(files, uses, source), err(level = "warn"))]
pub(in crate::etiquettes::internal_error_chain::apply) fn collect_getter_storage(
    files: &[crate::FileSnapshot],
    uses: &[super::super::getter_uses::GetterUseFact],
    source: &crate::ir::SourceErrorApply,
) -> crate::CordialResult<GetterFiles> {
    let mut names = Names::default();
    for file in files {
        names.extend(&source.getters_for(file)?.names);
    }
    let mut facts = GetterFiles::new();
    for file in files {
        let fields = facts.entry(file.path().to_path_buf()).or_default();
        for contract in &source.getters_for(file)?.fields {
            let super::GetterFieldContract {
                owner,
                field,
                storage,
            } = contract;
            let blocked = uses.iter().any(|usage| usage.blocks(file.path(), owner));
            let storage = match storage {
                super::GetterContract::Owned { insertion } => {
                    let alias = names.reserve(owner, field);
                    GetterStorageFacts {
                        edits: insertion
                            .as_ref()
                            .filter(|_| !blocked)
                            .map(|insertion| insertion.edits(owner, field, &alias)),
                        alias,
                        target: ErrorFieldStrategy::Borrowed,
                    }
                }
                super::GetterContract::Generated { alias, edits } => GetterStorageFacts {
                    alias: alias.clone(),
                    edits: edits
                        .clone()
                        .filter(|_| !blocked && names.count(alias) == 2),
                    target: ErrorFieldStrategy::Owned,
                },
                super::GetterContract::Borrowed => GetterStorageFacts {
                    alias: String::new(),
                    edits: None,
                    target: ErrorFieldStrategy::Owned,
                },
            };
            fields.insert((owner.clone(), field.clone()), storage);
        }
    }
    Ok(facts)
}

impl super::GetterContractFacts {
    #[instrument(level = "debug", skip(file))]
    #[hotpath::measure(impl_type = "GetterContractFacts")]
    pub(crate) fn collect(file: &SharedParsedFile) -> Self {
        let mut fields = Vec::new();
        for item in &file.syntax().items {
            let Item::Struct(item) = item else { continue };
            if !has_getters(file, item) {
                continue;
            }
            for field in &item.fields {
                let Some(field_name) = field.ident.as_ref().map(ToString::to_string) else {
                    continue;
                };
                if field_name != "file" || getter_skipped(field) {
                    continue;
                }
                let owner = item.ident.to_string();
                let storage = if super::super::is_string(&field.ty) {
                    super::GetterContract::Owned {
                        insertion: forward(file, item, field),
                    }
                } else if let Some(alias) = generated_alias(file, &field.ty, &owner, &field_name) {
                    super::GetterContract::Generated {
                        alias: alias.ident.to_string(),
                        edits: reverse(file, item, alias, field),
                    }
                } else if super::super::is_static_str(&field.ty) {
                    super::GetterContract::Borrowed
                } else {
                    continue;
                };
                fields.push(super::GetterFieldContract {
                    owner,
                    field: field_name,
                    storage,
                });
            }
        }
        Self {
            names: Names::collect(file),
            fields,
        }
    }
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

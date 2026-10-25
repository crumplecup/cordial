//! Lifetime closure for a file-local private error graph.

use std::collections::{BTreeMap, BTreeSet};

use syn::visit::Visit;
use syn::{GenericParam, Item, ItemEnum, ItemStruct, Type};
use tracing::instrument;

use crate::config::ErrorFieldStrategy;
use crate::loader::SharedParsedFile;

use super::super::Edit;
use super::types::{is_copy_shape, storage_shape, type_name};

mod edits;
mod provenance;

pub(super) use provenance::{anchored_parameters, constructor_origin};

#[derive(derive_getters::Getters)]
pub(super) struct GraphLifetimePlan {
    lifetime: String,
    names: BTreeSet<String>,
}

impl GraphLifetimePlan {
    #[instrument(level = "trace", skip(self, file, strategy, storage_edits))]
    pub(super) fn edits(
        &self,
        file: &SharedParsedFile,
        strategy: ErrorFieldStrategy,
        storage_edits: &[Edit],
    ) -> Option<Vec<Edit>> {
        edits::collect(self, file, strategy, storage_edits, false)
    }

    #[instrument(level = "trace", skip(self, file, strategy, storage_edits))]
    pub(super) fn distributed_edits(
        &self,
        file: &SharedParsedFile,
        strategy: ErrorFieldStrategy,
        storage_edits: &[Edit],
    ) -> Option<Vec<Edit>> {
        edits::collect(self, file, strategy, storage_edits, true)
    }
}

#[instrument(level = "debug", skip(file, strategy, names, structs, enums))]
pub(super) fn select(
    file: &SharedParsedFile,
    strategy: ErrorFieldStrategy,
    names: &BTreeSet<String>,
    structs: &BTreeMap<String, &ItemStruct>,
    enums: &BTreeMap<String, &ItemEnum>,
) -> Option<GraphLifetimePlan> {
    select_across(std::slice::from_ref(file), strategy, names, structs, enums)
}

#[instrument(level = "debug", skip(files, strategy, names, structs, enums))]
pub(super) fn select_across(
    files: &[SharedParsedFile],
    strategy: ErrorFieldStrategy,
    names: &BTreeSet<String>,
    structs: &BTreeMap<String, &ItemStruct>,
    enums: &BTreeMap<String, &ItemEnum>,
) -> Option<GraphLifetimePlan> {
    match strategy {
        ErrorFieldStrategy::Borrowed => {
            let changing: BTreeSet<_> = names
                .iter()
                .filter(|name| {
                    structs.get(*name).is_some_and(|item| {
                        item.generics.params.is_empty()
                            && fields_need_lifetime(&item.fields, strategy)
                    }) || enums.get(*name).is_some_and(|item| {
                        item.generics.params.is_empty()
                            && item
                                .variants
                                .iter()
                                .any(|variant| fields_need_lifetime(&variant.fields, strategy))
                    })
                })
                .cloned()
                .collect();
            if changing.is_empty() {
                return None;
            }
            let mut anchors = BTreeSet::new();
            for item in files.iter().flat_map(|file| &file.syntax().items) {
                let Item::Impl(implementation) = item else {
                    continue;
                };
                if !type_name(&implementation.self_ty).is_some_and(|name| changing.contains(&name))
                {
                    continue;
                }
                for member in &implementation.items {
                    let syn::ImplItem::Fn(method) = member else {
                        continue;
                    };
                    let [GenericParam::Lifetime(parameter)] = method
                        .sig
                        .generics
                        .params
                        .iter()
                        .collect::<Vec<_>>()
                        .as_slice()
                    else {
                        continue;
                    };
                    let lifetime = parameter.lifetime.ident.to_string();
                    if parameter.attrs.is_empty()
                        && parameter.bounds.is_empty()
                        && matches!(&method.sig.output, syn::ReturnType::Type(_, ty)
                            if matches!(&**ty, Type::Path(path) if path.path.is_ident("Self")))
                        && method
                            .sig
                            .inputs
                            .iter()
                            .all(|input| matches!(input, syn::FnArg::Typed(_)))
                        && method
                            .sig
                            .inputs
                            .iter()
                            .any(|input| contains_lifetime_arg(input, &lifetime))
                    {
                        anchors.insert(lifetime);
                    }
                }
            }
            if anchors.len() != 1 {
                return None;
            }
            let lifetime = anchors.into_iter().next()?;
            Some(GraphLifetimePlan {
                lifetime,
                names: changing,
            })
        }
        ErrorFieldStrategy::Owned => {
            let mut changing = BTreeSet::new();
            let mut lifetimes = BTreeSet::new();
            for name in names {
                if let Some(item) = structs.get(name) {
                    let Some(lifetime) = one_lifetime(&item.generics) else {
                        continue;
                    };
                    if item.generics.where_clause.is_some()
                        || !fields_need_lifetime(&item.fields, strategy)
                        || has_retained_lifetime(&item.fields, &lifetime)
                    {
                        return None;
                    }
                    changing.insert(name.clone());
                    lifetimes.insert(lifetime);
                } else if let Some(item) = enums.get(name) {
                    let Some(lifetime) = one_lifetime(&item.generics) else {
                        continue;
                    };
                    if item.generics.where_clause.is_some()
                        || item
                            .variants
                            .iter()
                            .any(|variant| has_retained_lifetime(&variant.fields, &lifetime))
                    {
                        return None;
                    }
                    changing.insert(name.clone());
                    lifetimes.insert(lifetime);
                }
            }
            if changing.is_empty() || lifetimes.len() != 1 {
                return None;
            }
            Some(GraphLifetimePlan {
                lifetime: lifetimes.into_iter().next()?,
                names: changing,
            })
        }
    }
}

#[instrument(level = "trace", skip(fields, strategy))]
fn fields_need_lifetime(fields: &syn::Fields, strategy: ErrorFieldStrategy) -> bool {
    fields
        .iter()
        .any(|field| field_needs_lifetime(field, strategy))
}

#[instrument(level = "trace", skip(field, strategy))]
fn field_needs_lifetime(field: &syn::Field, strategy: ErrorFieldStrategy) -> bool {
    if field.ident.as_ref().is_some_and(|name| name == "file") || storage_shape(&field.ty).is_none()
    {
        return false;
    }
    match strategy {
        ErrorFieldStrategy::Borrowed => !is_copy_shape(&field.ty),
        ErrorFieldStrategy::Owned => {
            matches!(field.ty, Type::Reference(_))
                && (field.ident.is_none()
                    || field
                        .ident
                        .as_ref()
                        .is_some_and(|name| name == "kind" || name == "source")
                    || type_name(&field.ty).as_deref() == Some("str"))
        }
    }
}

#[instrument(level = "trace", skip(fields, lifetime))]
fn has_retained_lifetime(fields: &syn::Fields, lifetime: &str) -> bool {
    fields.iter().any(|field| {
        contains_lifetime_type(&field.ty, lifetime)
            && !field_needs_lifetime(field, ErrorFieldStrategy::Owned)
    })
}

#[instrument(level = "trace", skip(generics))]
fn one_lifetime(generics: &syn::Generics) -> Option<String> {
    let [GenericParam::Lifetime(parameter)] = generics.params.iter().collect::<Vec<_>>().as_slice()
    else {
        return None;
    };
    (parameter.attrs.is_empty() && parameter.bounds.is_empty())
        .then(|| parameter.lifetime.ident.to_string())
}

#[instrument(level = "trace", skip(value, lifetime))]
fn contains_lifetime_type(value: &Type, lifetime: &str) -> bool {
    let mut visitor = LifetimeUse {
        lifetime,
        found: false,
    };
    visitor.visit_type(value);
    visitor.found
}

#[instrument(level = "trace", skip(value, lifetime))]
fn contains_lifetime_arg(value: &syn::FnArg, lifetime: &str) -> bool {
    let mut visitor = LifetimeUse {
        lifetime,
        found: false,
    };
    visitor.visit_fn_arg(value);
    visitor.found
}

struct LifetimeUse<'a> {
    lifetime: &'a str,
    found: bool,
}

impl<'ast> Visit<'ast> for LifetimeUse<'_> {
    #[instrument(level = "debug", skip(self, lifetime))]
    fn visit_lifetime(&mut self, lifetime: &'ast syn::Lifetime) {
        self.found |= lifetime.ident == self.lifetime;
    }
}

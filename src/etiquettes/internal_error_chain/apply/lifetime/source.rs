//! Compatibility lifetime collection, issued only by the shared loader.
use super::super::{is_string, simple_type_name};
use super::{LifetimeFileFacts, planner::plan};
use crate::config::ErrorFieldStrategy;
use crate::loader::SharedParsedFile;
use std::collections::BTreeMap;
use syn::{Item, ItemFn, ItemImpl, ItemStruct, Type};
use tracing::instrument;

impl LifetimeFileFacts {
    #[instrument(level = "debug", skip(file))]
    pub(crate) fn collect(file: &SharedParsedFile) -> Self {
        let index = LifetimeFileIndex::new(file.syntax());
        let mut sites = Vec::new();
        for item in index.structs.values() {
            let syn::Fields::Named(fields) = &item.fields else {
                continue;
            };
            for field in &fields.named {
                let Some(field_name) = field.ident.as_ref().map(ToString::to_string) else {
                    continue;
                };
                let strategy = if is_string(&field.ty) {
                    ErrorFieldStrategy::Borrowed
                } else {
                    ErrorFieldStrategy::Owned
                };
                if let Some(mut facts) = plan(file, &index, item, &field_name, strategy) {
                    facts.field_name = field_name;
                    sites.push(facts);
                }
            }
        }
        Self { sites }
    }
}

/// Top-level syntax facts shared by every lifetime candidate in one file.
#[derive(derive_getters::Getters)]
pub(super) struct LifetimeFileIndex<'a> {
    structs: BTreeMap<String, &'a ItemStruct>,
    impls: BTreeMap<String, Vec<&'a ItemImpl>>,
    functions: Vec<&'a ItemFn>,
}

impl<'a> LifetimeFileIndex<'a> {
    #[instrument(level = "debug", skip(syntax))]
    pub(super) fn new(syntax: &'a syn::File) -> Self {
        let mut index = Self {
            structs: BTreeMap::new(),
            impls: BTreeMap::new(),
            functions: Vec::new(),
        };
        for item in &syntax.items {
            match item {
                Item::Struct(item) => {
                    index.structs.entry(item.ident.to_string()).or_insert(item);
                }
                Item::Impl(item) => {
                    if let Some(name) = simple_type_name(&item.self_ty) {
                        index.impls.entry(name).or_default().push(item);
                    }
                }
                Item::Fn(item) => index.functions.push(item),
                _ => {}
            }
        }
        index
    }
}

#[instrument(level = "trace", skip(ty))]
pub(super) fn scalar_copy_name(ty: &Type) -> Option<&syn::Ident> {
    let Type::Path(path) = ty else {
        return None;
    };
    if path.qself.is_some() || path.path.segments.len() != 1 {
        return None;
    }
    let segment = path.path.segments.first()?;
    (matches!(
        segment.ident.to_string().as_str(),
        "bool"
            | "char"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "f32"
            | "f64"
    ) && matches!(segment.arguments, syn::PathArguments::None))
    .then_some(&segment.ident)
}

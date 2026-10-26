//! A closed lifetime migration for one dynamic field and scalar context.

use std::collections::HashMap;

use syn::{Item, ItemFn, ItemImpl, ItemStruct, Type};
use tracing::instrument;

use crate::config::ErrorFieldStrategy;
use crate::loader::SharedParsedFile;

use super::inventory::ErrorConversionUnit;
use super::{Edit, simple_type_name};

mod forwarding;
mod methods;
mod planner;

use planner::plan;

/// Top-level syntax facts shared by every lifetime candidate in one file.
pub(super) struct LifetimeFileIndex<'a> {
    structs: HashMap<String, &'a ItemStruct>,
    impls: HashMap<String, Vec<&'a ItemImpl>>,
    functions: Vec<&'a ItemFn>,
}

impl<'a> LifetimeFileIndex<'a> {
    #[instrument(level = "debug", skip(syntax))]
    pub(super) fn new(syntax: &'a syn::File) -> Self {
        let mut index = Self {
            structs: HashMap::new(),
            impls: HashMap::new(),
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

/// Collect a special lifetime migration from the shared parsed tree.
#[instrument(level = "debug", skip(file, index, unit, strategy, initializer_edits))]
pub(super) fn plan_for_unit(
    file: &SharedParsedFile,
    index: &LifetimeFileIndex<'_>,
    unit: &ErrorConversionUnit,
    strategy: ErrorFieldStrategy,
    initializer_edits: Option<Vec<Edit>>,
) -> Option<Vec<Edit>> {
    let item = index.structs.get(unit.type_name())?;
    plan(
        file,
        index,
        item,
        unit.field_name(),
        unit,
        strategy,
        initializer_edits,
    )
}

#[instrument(level = "trace", skip(ty))]
fn scalar_copy_name(ty: &Type) -> Option<&syn::Ident> {
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

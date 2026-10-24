//! A closed lifetime migration for one dynamic field and scalar context.

use std::collections::HashMap;

use syn::{Item, ItemFn, ItemImpl, ItemStruct, Type};
use tracing::instrument;

use crate::config::ErrorFieldStrategy;
use crate::loader::SharedParsedFile;

use super::initializers::FieldKind;
use super::inventory::ErrorConversionUnit;
use super::{Edit, is_string, simple_type_name};

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
#[instrument(level = "debug", skip(file, index, unit, strategy))]
pub(super) fn plan_for_unit(
    file: &SharedParsedFile,
    index: &LifetimeFileIndex<'_>,
    unit: &ErrorConversionUnit,
    strategy: ErrorFieldStrategy,
) -> Option<Vec<Edit>> {
    let item = index.structs.get(unit.type_name())?;
    plan(file, index, item, unit.field_name(), unit, strategy)
}

#[instrument(level = "trace", skip(item, field, kind, strategy))]
pub(super) fn is_candidate(
    item: &ItemStruct,
    field: &syn::Field,
    kind: FieldKind,
    struct_lifetime: Option<&str>,
    strategy: ErrorFieldStrategy,
) -> bool {
    matches!(kind, FieldKind::StaticString)
        && struct_lifetime.is_none()
        && match strategy {
            ErrorFieldStrategy::Borrowed => is_string(&field.ty) && item.generics.params.is_empty(),
            ErrorFieldStrategy::Owned => matches!(&field.ty, Type::Reference(reference)
                if reference.lifetime.as_ref().is_some_and(|named| named.ident != "static")
                    && matches!(&*reference.elem, Type::Path(path) if path.path.is_ident("str"))),
        }
}

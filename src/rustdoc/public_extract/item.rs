//! Build [`ExtractedItem`] rows from rustdoc index/path summaries.

use tracing::instrument;

use super::stability::rustdoc_item_is_unstable;
use super::{ExtractedItem, ExtractedItemKind};

#[instrument(skip(krate, summary), fields(id = ?id))]
pub(super) fn build_inventory_item(
    krate: &rustdoc_types::Crate,
    id: &rustdoc_types::Id,
    summary: &rustdoc_types::ItemSummary,
) -> Option<ExtractedItem> {
    build_inventory_item_with_path(krate, id, summary.kind, summary.path.clone())
}
#[instrument(skip(krate), fields(id = ?id, kind = ?kind))]
pub(super) fn build_inventory_item_with_path(
    krate: &rustdoc_types::Crate,
    id: &rustdoc_types::Id,
    kind: rustdoc_types::ItemKind,
    path: Vec<String>,
) -> Option<ExtractedItem> {
    let kind = match kind {
        rustdoc_types::ItemKind::Struct => ExtractedItemKind::Struct,
        rustdoc_types::ItemKind::Enum => ExtractedItemKind::Enum,
        rustdoc_types::ItemKind::Trait => ExtractedItemKind::Trait,
        rustdoc_types::ItemKind::TypeAlias => ExtractedItemKind::TypeAlias,
        rustdoc_types::ItemKind::Function => ExtractedItemKind::Function,
        rustdoc_types::ItemKind::Primitive => ExtractedItemKind::Other,
        rustdoc_types::ItemKind::Module
        | rustdoc_types::ItemKind::Macro
        | rustdoc_types::ItemKind::Constant => return None,
        _ => return None,
    };

    let name = path.last().cloned().unwrap_or_default();
    if name.is_empty() {
        return None;
    }

    let index_item = krate.index.get(id);

    let (is_generic, _lifetime_params, _type_params) = index_item
        .map(|item| {
            let (_, g, lp, tp) = classify_item(item);
            (g, lp, tp)
        })
        .unwrap_or((false, vec![], vec![]));

    // For type aliases, extract the path of the aliased type so the coverage
    // checker can fall back to the underlying type's impl status.
    let alias_target = if kind == ExtractedItemKind::TypeAlias {
        index_item.and_then(|item| {
            if let rustdoc_types::ItemEnum::TypeAlias(ta) = &item.inner {
                alias_target_path(&ta.type_)
            } else {
                None
            }
        })
    } else {
        None
    };

    Some(ExtractedItem {
        path,
        kind: kind.to_inventory(),
        name,
        is_generic,
        alias_target,
        is_unstable: index_item.is_some_and(|item| rustdoc_item_is_unstable(krate, id, item)),
    })
}
/// Extract a human-readable path string from a rustdoc `Type`, used to record
/// what a type alias resolves to. Handles `ResolvedPath` (another named
/// item), `Primitive` (`dev_t -> u64`), and `RawPointer` (`HANDLE -> *mut
/// c_void`, recursing into the pointee so the result is still shape-matchable
/// by `type_has_trait_impl`'s `compound_primitive_shape` fallback even though
/// the pointee itself is a `ResolvedPath`, not a bare primitive). Returns
/// `None` for function pointers, tuples, and other complex types.
#[instrument(skip(ty))]
fn alias_target_path(ty: &rustdoc_types::Type) -> Option<String> {
    match ty {
        rustdoc_types::Type::ResolvedPath(p) => Some(p.path.clone()),
        rustdoc_types::Type::Primitive(name) => Some(name.clone()),
        rustdoc_types::Type::RawPointer { is_mutable, type_ } => {
            let pointee = alias_target_path(type_).unwrap_or_else(|| "_".to_string());
            let qualifier = if *is_mutable { "mut" } else { "const" };
            Some(format!("*{qualifier} {pointee}"))
        }
        _ => None,
    }
}
/// Map a rustdoc item to our [`ExtractedItemKind`], and extract generics info.
#[instrument(skip(item))]
fn classify_item(
    item: &rustdoc_types::Item,
) -> (ExtractedItemKind, bool, Vec<String>, Vec<String>) {
    match &item.inner {
        rustdoc_types::ItemEnum::Struct(s) => {
            let (lifetime_params, type_params) = extract_generic_params(&s.generics);
            let is_generic = !lifetime_params.is_empty() || !type_params.is_empty();
            (
                ExtractedItemKind::Struct,
                is_generic,
                lifetime_params,
                type_params,
            )
        }
        rustdoc_types::ItemEnum::Enum(e) => {
            let (lifetime_params, type_params) = extract_generic_params(&e.generics);
            let is_generic = !lifetime_params.is_empty() || !type_params.is_empty();
            (
                ExtractedItemKind::Enum,
                is_generic,
                lifetime_params,
                type_params,
            )
        }
        rustdoc_types::ItemEnum::Trait(t) => {
            let (lifetime_params, type_params) = extract_generic_params(&t.generics);
            let is_generic = !lifetime_params.is_empty() || !type_params.is_empty();
            (
                ExtractedItemKind::Trait,
                is_generic,
                lifetime_params,
                type_params,
            )
        }
        rustdoc_types::ItemEnum::TypeAlias(t) => {
            let (lifetime_params, type_params) = extract_generic_params(&t.generics);
            let is_generic = !lifetime_params.is_empty() || !type_params.is_empty();
            (
                ExtractedItemKind::TypeAlias,
                is_generic,
                lifetime_params,
                type_params,
            )
        }
        rustdoc_types::ItemEnum::Function(_) => {
            (ExtractedItemKind::Function, false, vec![], vec![])
        }
        rustdoc_types::ItemEnum::Macro(_) => (ExtractedItemKind::Macro, false, vec![], vec![]),
        rustdoc_types::ItemEnum::Constant { .. } => {
            (ExtractedItemKind::Constant, false, vec![], vec![])
        }
        rustdoc_types::ItemEnum::Module(_) => (ExtractedItemKind::Module, false, vec![], vec![]),
        _ => (ExtractedItemKind::Other, false, vec![], vec![]),
    }
}
/// Extract lifetime and type parameter names from a [`Generics`] block.
#[instrument(skip(generics))]
fn extract_generic_params(generics: &rustdoc_types::Generics) -> (Vec<String>, Vec<String>) {
    let mut lifetime_params = Vec::new();
    let mut type_params = Vec::new();

    for param in &generics.params {
        match &param.kind {
            rustdoc_types::GenericParamDefKind::Lifetime { .. } => {
                lifetime_params.push(param.name.clone())
            }
            rustdoc_types::GenericParamDefKind::Type { .. } => type_params.push(param.name.clone()),
            _ => {}
        }
    }

    (lifetime_params, type_params)
}

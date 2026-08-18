use std::collections::HashSet;

use rustdoc_types::{GenericArg, GenericArgs, ItemEnum, Type};

use super::impls::impl_target_path;
use super::inventory::RustdocInventory;

use tracing::instrument;
/// Wrapper type paired with the foreign type it wraps via `From<T>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrenchcoatPair {
    pub wrapper_path: String,
    pub foreign_path: String,
}

/// Collect `(wrapper, foreign)` pairs from `From` impls on elicitation-style wrappers.
#[instrument(level = "debug", skip(inventory))]
pub fn collect_trenchcoat_pairs(inventory: &RustdocInventory) -> Vec<TrenchcoatPair> {
    let mut pairs = Vec::new();
    let mut seen = HashSet::new();

    for item in inventory.krate.index.values() {
        let ItemEnum::Impl(impl_item) = &item.inner else {
            continue;
        };
        let Some(trait_) = &impl_item.trait_ else {
            continue;
        };
        if trait_.path.rsplit("::").next() != Some("From") {
            continue;
        }
        let Some(wrapper_path) = impl_target_path(&inventory.krate, &impl_item.for_) else {
            continue;
        };
        if !is_wrapper_path(&wrapper_path) {
            continue;
        }
        let Some(foreign_path) = from_foreign_path(trait_) else {
            continue;
        };
        let key = (wrapper_path.clone(), foreign_path.clone());
        if seen.insert(key) {
            pairs.push(TrenchcoatPair {
                wrapper_path,
                foreign_path,
            });
        }
    }

    pairs.sort_by(|a, b| {
        a.wrapper_path
            .cmp(&b.wrapper_path)
            .then(a.foreign_path.cmp(&b.foreign_path))
    });
    pairs
}

fn from_foreign_path(trait_: &rustdoc_types::Path) -> Option<String> {
    let GenericArgs::AngleBracketed { args, .. } = trait_.args.as_deref()? else {
        return None;
    };
    let GenericArg::Type(Type::ResolvedPath(path)) = args.first()? else {
        return None;
    };
    Some(path.path.clone())
}

fn is_wrapper_path(path: &str) -> bool {
    path.contains("Wrapper")
        || path.ends_with("Coat")
        || path.contains("Trenchcoat")
        || path.contains("Elicit")
}

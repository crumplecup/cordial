//! Public method and trait-impl maps extracted from rustdoc JSON.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use rustdoc_types::{Crate, ItemEnum, Type, Visibility};
use tracing::instrument;

use crate::error::CordialResult;

use super::RustdocInventory;
use super::public_extract::{collect_public_same_crate_reexport_aliases, item_is_public};

#[instrument(level = "trace", ret)]
fn is_user_facing_method(name: &str) -> bool {
    !name.starts_with("__")
        && name != "assert_receiver_is_total_eq"
        && name != "assert_fields_are_eq"
}

/// Collect public method names keyed by canonical type path from a rustdoc crate.
#[instrument(level = "debug", skip(krate))]
pub fn collect_type_methods_from_krate(krate: &Crate) -> HashMap<String, BTreeSet<String>> {
    let mut methods: HashMap<String, BTreeSet<String>> = HashMap::new();

    for item in krate.index.values() {
        let ItemEnum::Impl(impl_item) = &item.inner else {
            continue;
        };
        if impl_item.is_synthetic || impl_item.blanket_impl.is_some() {
            continue;
        }
        let Type::ResolvedPath(resolved) = &impl_item.for_ else {
            continue;
        };
        let Some(summary) = krate.paths.get(&resolved.id) else {
            continue;
        };
        let type_path = summary.path.join("::");
        let entry = methods.entry(type_path).or_default();
        let in_trait_impl = impl_item.trait_.is_some();

        for method_id in &impl_item.items {
            let Some(method_item) = krate.index.get(method_id) else {
                continue;
            };
            let visible = if in_trait_impl {
                matches!(
                    method_item.visibility,
                    Visibility::Public | Visibility::Default
                )
            } else {
                item_is_public(method_item)
            };
            let name = method_item.name.as_deref().unwrap_or("?");
            let is_fn = matches!(method_item.inner, ItemEnum::Function(_));
            if !visible || !is_fn || !is_user_facing_method(name) {
                continue;
            }
            entry.insert(name.to_owned());
        }
    }

    methods
}

/// Collect method maps from a parsed inventory (uses embedded `krate` JSON).
#[instrument(level = "debug", skip(inventory))]
pub fn collect_type_methods_from_inventory(
    inventory: &RustdocInventory,
) -> HashMap<String, BTreeSet<String>> {
    collect_type_methods_from_krate(&inventory.krate)
}

/// Collect method maps by reading rustdoc JSON from disk.
#[instrument(level = "debug", err(level = "warn"))]
pub fn collect_type_methods(json_path: &Path) -> CordialResult<HashMap<String, BTreeSet<String>>> {
    let content = std::fs::read_to_string(json_path)?;
    let krate: Crate = serde_json::from_str(&content)?;
    Ok(collect_type_methods_from_krate(&krate))
}

/// Build `trait_path → implementing type bare names` from rustdoc JSON.
#[instrument(level = "debug", skip(krate))]
pub fn collect_trait_impl_map_from_krate(krate: &Crate) -> HashMap<String, BTreeSet<String>> {
    let own_crate = krate
        .index
        .get(&krate.root)
        .and_then(|item| item.name.as_deref())
        .unwrap_or("")
        .replace('-', "_");

    let aliases = collect_public_same_crate_reexport_aliases(krate, &own_crate, false);
    let alias_path: HashMap<&rustdoc_types::Id, Vec<String>> = aliases
        .iter()
        .map(|(id, item)| (id, item.path.clone()))
        .collect();

    let mut map: HashMap<String, BTreeSet<String>> = HashMap::new();

    for item in krate.index.values() {
        let ItemEnum::Impl(impl_item) = &item.inner else {
            continue;
        };
        if impl_item.is_synthetic || impl_item.blanket_impl.is_some() {
            continue;
        }
        let Some(trait_) = &impl_item.trait_ else {
            continue;
        };

        let trait_path = alias_path
            .get(&trait_.id)
            .map(|path| path.join("::"))
            .or_else(|| {
                krate
                    .paths
                    .get(&trait_.id)
                    .map(|summary| summary.path.join("::"))
            })
            .unwrap_or_else(|| trait_.path.clone());

        let Type::ResolvedPath(resolved) = &impl_item.for_ else {
            continue;
        };
        let Some(summary) = krate.paths.get(&resolved.id) else {
            continue;
        };
        let Some(bare_name) = summary.path.last().cloned() else {
            continue;
        };

        map.entry(trait_path).or_default().insert(bare_name);
    }

    map
}

/// Collect trait impl map from inventory.
#[instrument(level = "debug", skip(inventory))]
pub fn collect_trait_impl_map_from_inventory(
    inventory: &RustdocInventory,
) -> HashMap<String, BTreeSet<String>> {
    collect_trait_impl_map_from_krate(&inventory.krate)
}

/// Collect trait impl map.
#[instrument(level = "debug", err(level = "warn"))]
pub fn collect_trait_impl_map(
    json_path: &Path,
) -> CordialResult<HashMap<String, BTreeSet<String>>> {
    let content = std::fs::read_to_string(json_path)?;
    let krate: Crate = serde_json::from_str(&content)?;
    Ok(collect_trait_impl_map_from_krate(&krate))
}

/// Look up the method set for a type path, with bare-name suffix fallback.
#[instrument(level = "debug", skip(methods))]
pub fn methods_for_type_path(
    item_path: &str,
    methods: &HashMap<String, BTreeSet<String>>,
) -> BTreeSet<String> {
    if let Some(found) = methods.get(item_path) {
        return found.clone();
    }
    let bare = item_path.rsplit("::").next().unwrap_or(item_path);
    let suffix = format!("::{bare}");
    let crate_prefix = item_path.split("::").next().unwrap_or("");
    let matches: Vec<(&String, &BTreeSet<String>)> = methods
        .iter()
        .filter(|(path, _)| *path == bare || path.ends_with(&suffix))
        .collect();
    match matches.len() {
        0 => BTreeSet::new(),
        1 => matches[0].1.clone(),
        _ => matches
            .iter()
            .find(|(path, _)| path.split("::").next() == Some(crate_prefix))
            .map(|(_, methods)| (*methods).clone())
            .unwrap_or_else(|| matches[0].1.clone()),
    }
}

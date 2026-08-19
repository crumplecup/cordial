//! Public rustdoc inventory extraction (ported from elicit_doc `collect/inventory`).

use std::collections::HashSet;

use rustdoc_types::Crate;
use tracing::{debug, instrument};

use super::InventoryItemKind;

mod generics;
mod item;
mod reexport;
mod signature;
mod stability;
mod type_walk;

pub(super) use reexport::collect_public_same_crate_reexport_aliases;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedItem {
    pub path: Vec<String>,
    pub kind: InventoryItemKind,
    pub name: String,
    pub is_generic: bool,
    pub alias_target: Option<String>,
    pub is_unstable: bool,
}

impl ExtractedItem {
    #[instrument(level = "trace", skip(self))]
    pub fn path_str(&self) -> String {
        self.path.join("::")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ExtractedItemKind {
    Struct,
    Enum,
    Trait,
    TypeAlias,
    Function,
    Macro,
    Constant,
    Module,
    Other,
}

impl ExtractedItemKind {
    #[instrument(level = "debug", skip(self))]
    pub(super) fn to_inventory(self) -> InventoryItemKind {
        match self {
            Self::Struct => InventoryItemKind::Struct,
            Self::Enum => InventoryItemKind::Enum,
            Self::Trait => InventoryItemKind::Trait,
            Self::TypeAlias => InventoryItemKind::TypeAlias,
            Self::Function => InventoryItemKind::Function,
            _ => InventoryItemKind::Other,
        }
    }
}

#[instrument(level = "debug", skip(krate))]
pub fn extract_public_items(
    krate: &Crate,
    own_crate: &str,
    prefix_match: bool,
) -> Vec<ExtractedItem> {
    extract_items(krate, own_crate, prefix_match)
}

/// Extract all public items from a rustdoc [`Crate`] into our flat [`ExtractedItem`] list.
///
/// For re-exporting umbrella crates (like `bevy`) the `index` only contains
/// a handful of module items while all re-exported items live in `paths`.
/// We therefore build the inventory from `paths` and look up the `index` entry
/// only for additional generics detail when available.
///
/// `prefix_match`: when `true`, items are accepted if their first path segment
/// **starts with** `own_crate` (e.g. `"bevy"` accepts `bevy_ecs::*`, `bevy_math::*`).
/// When `false`, the first segment must equal `own_crate` exactly.
#[instrument(level = "debug", skip(krate))]
#[doc(hidden)]
pub fn extract_items(
    krate: &rustdoc_types::Crate,
    own_crate: &str,
    prefix_match: bool,
) -> Vec<ExtractedItem> {
    let mut items = Vec::new();
    let mut seen_paths = HashSet::new();
    // Rustdoc JSON paths always use underscores even when the Cargo.toml package
    // name is hyphenated (e.g. "geo-types" → "geo_types").
    let own_crate_normalized = own_crate.replace('-', "_");
    let own_crate_key = own_crate_normalized.as_str();
    let public_reexport_aliases =
        reexport::collect_public_same_crate_reexport_aliases(krate, own_crate_key, prefix_match);
    let public_module_paths = collect_public_module_paths(krate, own_crate_key, prefix_match);

    for item in public_reexport_aliases.values() {
        seen_paths.insert(item.path_str());
        items.push(item.clone());
    }

    for (id, summary) in &krate.paths {
        if !path_matches_scope(&summary.path, own_crate_key, prefix_match) {
            continue;
        }
        if public_reexport_aliases.contains_key(id) {
            debug!(
                target_path = %summary.path.join("::"),
                "skipping canonical same-crate path in favor of public reexport alias"
            );
            continue;
        }

        let Some(item) = item::build_inventory_item(krate, id, summary) else {
            continue;
        };
        if !item_path_is_publicly_reachable(&item, &public_module_paths) {
            debug!(
                item_path = %item.path_str(),
                "skipping non-publicly-reachable canonical path"
            );
            continue;
        }
        seen_paths.insert(item.path_str());
        items.push(item);
    }

    for item in reexport::collect_public_reexport_dependency_items(
        krate,
        own_crate_key,
        prefix_match,
        &seen_paths,
    ) {
        if seen_paths.insert(item.path_str()) {
            items.push(item);
        }
    }

    for item in signature::collect_public_signature_dependency_items(
        krate,
        own_crate_key,
        prefix_match,
        &seen_paths,
    ) {
        if seen_paths.insert(item.path_str()) {
            items.push(item);
        }
    }

    items.sort_by(|a, b| a.path.cmp(&b.path));
    tracing::debug!(count = items.len(), "extracted items");
    items
}

#[instrument(level = "debug", skip(path))]
pub(super) fn path_matches_scope(path: &[String], own_crate_key: &str, prefix_match: bool) -> bool {
    path.first()
        .map(|segment| {
            if prefix_match {
                segment.starts_with(own_crate_key)
            } else {
                segment == own_crate_key
            }
        })
        .unwrap_or(false)
}

#[instrument(level = "debug", skip(krate))]
fn collect_public_module_paths(
    krate: &rustdoc_types::Crate,
    own_crate_key: &str,
    prefix_match: bool,
) -> HashSet<String> {
    krate
        .paths
        .iter()
        .filter_map(|(id, summary)| {
            if !path_matches_scope(&summary.path, own_crate_key, prefix_match)
                || summary.kind != rustdoc_types::ItemKind::Module
            {
                return None;
            }
            let item = krate.index.get(id)?;
            item_is_public(item).then_some(summary.path.join("::"))
        })
        .collect()
}

#[instrument(level = "debug", skip(item, public_module_paths))]
fn item_path_is_publicly_reachable(
    item: &ExtractedItem,
    public_module_paths: &HashSet<String>,
) -> bool {
    if item.path.len() <= 2 {
        return true;
    }

    for idx in 1..item.path.len() - 1 {
        let module_path = item.path[..=idx].join("::");
        if !public_module_paths.contains(&module_path) {
            debug!(
                item_path = %item.path_str(),
                missing_public_module = %module_path,
                "canonical path is not publicly reachable"
            );
            return false;
        }
    }

    true
}

#[instrument(level = "debug", skip(item))]
pub(super) fn item_is_public(item: &rustdoc_types::Item) -> bool {
    matches!(item.visibility, rustdoc_types::Visibility::Public)
}

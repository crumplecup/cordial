//! Same-crate public re-export aliases and foreign re-export dependencies.

use std::collections::{HashMap, HashSet};

use tracing::{debug, instrument};

use super::item::build_inventory_item;
use super::item::build_inventory_item_with_path;
use super::{ExtractedItem, item_is_public, path_matches_scope};

/// Build a map from every item ID to the path of its direct parent module.
///
/// Rustdoc JSON `Use` items for `pub use` re-exports are often absent from
/// `krate.paths`.  Walking the module tree gives us an alternative way to
/// infer the re-export path: `parent_module_path + use_item.name`.
#[instrument(skip(krate))]
fn build_parent_module_paths(
    krate: &rustdoc_types::Crate,
) -> HashMap<rustdoc_types::Id, Vec<String>> {
    let mut parent_paths: HashMap<rustdoc_types::Id, Vec<String>> = HashMap::new();
    for (mod_id, item) in &krate.index {
        let rustdoc_types::ItemEnum::Module(module) = &item.inner else {
            continue;
        };
        let Some(summary) = krate.paths.get(mod_id) else {
            continue;
        };
        for child_id in &module.items {
            parent_paths.insert(*child_id, summary.path.clone());
        }
    }
    parent_paths
}

#[instrument(skip(krate), fields(own_crate_key, prefix_match))]
pub(in crate::rustdoc) fn collect_public_same_crate_reexport_aliases(
    krate: &rustdoc_types::Crate,
    own_crate_key: &str,
    prefix_match: bool,
) -> HashMap<rustdoc_types::Id, ExtractedItem> {
    let parent_module_paths = build_parent_module_paths(krate);
    let mut aliases: HashMap<rustdoc_types::Id, ExtractedItem> = HashMap::new();

    for (id, item) in &krate.index {
        let rustdoc_types::ItemEnum::Use(use_item) = &item.inner else {
            continue;
        };
        if !item_is_public(item) {
            continue;
        }

        // Prefer the path recorded in krate.paths; fall back to inferring from
        // the parent module when the Use item itself has no paths entry (common
        // for `pub use` re-exports in crates like chrono that restructure their
        // public API through private intermediate modules).
        let use_path: Vec<String> = if let Some(summary) = krate.paths.get(id) {
            summary.path.clone()
        } else {
            let Some(parent_path) = parent_module_paths.get(id) else {
                continue;
            };
            if !path_matches_scope(parent_path, own_crate_key, prefix_match) {
                continue;
            }
            let mut path = parent_path.clone();
            path.push(use_item.name.clone());
            path
        };

        if !path_matches_scope(&use_path, own_crate_key, prefix_match) {
            continue;
        }

        let Some(target_id) = &use_item.id else {
            continue;
        };
        let Some(target_summary) = krate.paths.get(target_id) else {
            continue;
        };
        if !path_matches_scope(&target_summary.path, own_crate_key, prefix_match) {
            continue;
        }

        let Some(alias_item) =
            build_inventory_item_with_path(krate, target_id, target_summary.kind, use_path)
        else {
            continue;
        };

        match aliases.entry(*target_id) {
            std::collections::hash_map::Entry::Vacant(slot) => {
                debug!(
                    target_path = %target_summary.path.join("::"),
                    alias_path = %alias_item.path_str(),
                    "recorded same-crate public reexport alias"
                );
                slot.insert(alias_item);
            }
            std::collections::hash_map::Entry::Occupied(mut slot) => {
                if item_path_preferred_over(&alias_item.path, &slot.get().path) {
                    debug!(
                        target_path = %target_summary.path.join("::"),
                        previous_alias = %slot.get().path_str(),
                        alias_path = %alias_item.path_str(),
                        "replaced same-crate public reexport alias with shorter public path"
                    );
                    slot.insert(alias_item);
                }
            }
        }
    }

    aliases
}
#[instrument(skip(krate, existing_paths), fields(own_crate_key, prefix_match, existing_count = existing_paths.len()))]
pub(super) fn collect_public_reexport_dependency_items(
    krate: &rustdoc_types::Crate,
    own_crate_key: &str,
    prefix_match: bool,
    existing_paths: &HashSet<String>,
) -> Vec<ExtractedItem> {
    let mut discovered = Vec::new();
    let mut seen = existing_paths.clone();

    for (id, item) in &krate.index {
        let rustdoc_types::ItemEnum::Use(use_item) = &item.inner else {
            continue;
        };
        if !item_is_public(item) {
            continue;
        }
        let Some(use_summary) = krate.paths.get(id) else {
            continue;
        };
        if !path_matches_scope(&use_summary.path, own_crate_key, prefix_match) {
            continue;
        }
        let Some(target_id) = &use_item.id else {
            continue;
        };
        let Some(target_summary) = krate.paths.get(target_id) else {
            continue;
        };

        let target_path = target_summary.path.join("::");
        if path_matches_scope(&target_summary.path, own_crate_key, prefix_match)
            || target_path.starts_with("std::")
            || target_path.starts_with("core::")
            || target_path.starts_with("alloc::")
        {
            continue;
        }

        if seen.insert(target_path)
            && let Some(item) = build_inventory_item(krate, target_id, target_summary)
        {
            discovered.push(item);
        }
    }

    discovered
}
#[instrument(skip(candidate, incumbent))]
fn item_path_preferred_over(candidate: &[String], incumbent: &[String]) -> bool {
    candidate.len() < incumbent.len()
        || (candidate.len() == incumbent.len() && candidate < incumbent)
}

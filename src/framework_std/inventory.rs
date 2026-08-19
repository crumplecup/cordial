//! Load std-family inventories from cached rustdoc JSON.

use std::path::Path;

use rustdoc_types::Crate;
use tracing::instrument;

use crate::error::{CordialError, CordialResult};
use crate::framework_std::StdInventoryItem;
use crate::rustdoc::{ExtractedItem, extract_public_items};
use crate::store::SysrootCache;

pub const FRAMEWORK_STD_SOURCES: &[&str] = &["std", "core", "alloc"];

/// Convert extracted public inventory rows into framework std items.
#[instrument(level = "debug", skip(items))]
pub fn std_items_from_extracted(items: &[ExtractedItem]) -> Vec<StdInventoryItem> {
    items
        .iter()
        .map(|item| StdInventoryItem {
            path: item.path_str(),
            kind: item.kind,
            is_generic: item.is_generic,
            is_unstable: item.is_unstable,
            alias_target: item.alias_target.clone(),
        })
        .collect()
}

/// Load one std-family inventory from the shared sysroot cache.
#[instrument(level = "info", skip(sysroot), fields(crate_name = crate_name), err(level = "warn"))]
pub fn load_std_inventory_from_sysroot(
    sysroot: &SysrootCache,
    crate_name: &str,
) -> CordialResult<Vec<StdInventoryItem>> {
    let path = sysroot.rustdoc_cache_path(crate_name);
    load_std_inventory_from_json(&path, crate_name)
}

/// Load one std-family inventory from a rustdoc JSON file.
#[instrument(level = "info", fields(crate_name = crate_name), err(level = "warn"))]
pub fn load_std_inventory_from_json(
    json_path: &Path,
    crate_name: &str,
) -> CordialResult<Vec<StdInventoryItem>> {
    if !json_path.is_file() {
        return Err(CordialError::invariant(format!(
            "rustdoc JSON not found for `{crate_name}` at {}",
            json_path.display()
        )));
    }
    let content = std::fs::read_to_string(json_path)?;
    let krate: Crate = serde_json::from_str(&content)?;
    let items = extract_public_items(&krate, crate_name, false);
    Ok(std_items_from_extracted(&items))
}

/// Load merged std/core/alloc inventories from the sysroot cache.
#[instrument(level = "info", skip(sysroot), err(level = "warn"))]
pub fn load_merged_std_inventory(sysroot: &SysrootCache) -> CordialResult<Vec<StdInventoryItem>> {
    let mut inventories = Vec::new();
    for source in FRAMEWORK_STD_SOURCES {
        match load_std_inventory_from_sysroot(sysroot, source) {
            Ok(items) => inventories.push(items),
            Err(err) => {
                tracing::debug!(source, error = %err, "std source not cached");
            }
        }
    }
    if inventories.is_empty() {
        return Err(CordialError::invariant(
            "no std-family rustdoc cache found under ~/.cordial/sysroot — run `cordial build sysroot`",
        ));
    }
    Ok(crate::framework_std::types::merge_std_inventory_items(
        &inventories,
    ))
}

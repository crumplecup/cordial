use std::collections::{HashMap, HashSet};
use std::path::Path;

use rustdoc_types::{Crate, ItemKind as RustdocKind};

use crate::error::CordialResult;

use tracing::instrument;
/// Kind of item extracted from rustdoc JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InventoryItemKind {
    Struct,
    Enum,
    Trait,
    TypeAlias,
    Function,
    Other,
}

impl InventoryItemKind {
    fn from_rustdoc(kind: RustdocKind) -> Self {
        match kind {
            RustdocKind::Struct => Self::Struct,
            RustdocKind::Enum => Self::Enum,
            RustdocKind::Trait => Self::Trait,
            RustdocKind::TypeAlias => Self::TypeAlias,
            RustdocKind::Function => Self::Function,
            _ => Self::Other,
        }
    }

    #[instrument(level = "trace", skip(self), ret)]
    pub fn is_type(self) -> bool {
        matches!(self, Self::Struct | Self::Enum | Self::TypeAlias)
    }

    #[instrument(level = "trace", skip(self))]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Struct => "Struct",
            Self::Enum => "Enum",
            Self::Trait => "Trait",
            Self::TypeAlias => "TypeAlias",
            Self::Function => "Function",
            Self::Other => "Other",
        }
    }
}

/// One public type or trait from a crate inventory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RustdocItem {
    pub path: String,
    pub name: String,
    pub kind: InventoryItemKind,
    pub is_public: bool,
}

/// Parsed rustdoc inventory for one crate.
#[derive(Debug, Clone)]
pub struct RustdocInventory {
    pub crate_name: String,
    pub crate_version: String,
    pub items: Vec<RustdocItem>,
    pub krate: Crate,
}

impl RustdocInventory {
    #[instrument(level = "trace", skip(self))]
    pub fn type_items(&self) -> impl Iterator<Item = &RustdocItem> {
        self.items.iter().filter(|item| item.kind.is_type())
    }
}

/// Parse rustdoc JSON into a flat inventory of public crate items.
#[instrument(level = "debug", err(level = "warn"))]
pub fn parse_rustdoc_json(json_path: &Path, crate_name: &str) -> CordialResult<RustdocInventory> {
    let content = std::fs::read_to_string(json_path)?;
    let krate: Crate = serde_json::from_str(&content)?;
    let crate_version = krate
        .crate_version
        .clone()
        .unwrap_or_else(|| "unknown".to_string());
    let items = extract_items(&krate, crate_name);
    Ok(RustdocInventory {
        crate_name: crate_name.to_string(),
        crate_version,
        items,
        krate,
    })
}

fn extract_items(krate: &Crate, own_crate: &str) -> Vec<RustdocItem> {
    let own_key = own_crate.replace('-', "_");
    let mut seen = HashSet::new();
    let mut items = Vec::new();

    for (id, summary) in &krate.paths {
        if !path_in_crate(&summary.path, &own_key) {
            continue;
        }
        let Some(item) = krate.index.get(id) else {
            continue;
        };
        if !matches!(item.visibility, rustdoc_types::Visibility::Public) {
            continue;
        }
        let kind = InventoryItemKind::from_rustdoc(summary.kind);
        if kind == InventoryItemKind::Other {
            continue;
        }
        let path = summary.path.join("::");
        if !seen.insert(path.clone()) {
            continue;
        }
        items.push(RustdocItem {
            name: item.name.clone().unwrap_or_else(|| "item".to_string()),
            path,
            kind,
            is_public: true,
        });
    }

    items.sort_by(|a, b| a.path.cmp(&b.path));
    items
}

fn path_in_crate(path: &[String], own_key: &str) -> bool {
    path.first().is_some_and(|first| first == own_key)
}

/// Map rustdoc item kind to IR item kind.
#[instrument(level = "debug", skip(kind))]
pub fn ir_item_kind(kind: InventoryItemKind) -> crate::ir::ItemKind {
    match kind {
        InventoryItemKind::Struct => crate::ir::ItemKind::Struct,
        InventoryItemKind::Enum => crate::ir::ItemKind::Enum,
        InventoryItemKind::Trait => crate::ir::ItemKind::Trait,
        InventoryItemKind::TypeAlias => crate::ir::ItemKind::TypeAlias,
        InventoryItemKind::Function => crate::ir::ItemKind::Fn,
        InventoryItemKind::Other => crate::ir::ItemKind::Other,
    }
}

/// Build path normalisation map from private module paths to public inventory paths.
#[instrument(level = "debug", skip(inventory))]
pub fn canonical_to_public_map(inventory: &RustdocInventory) -> HashMap<String, String> {
    let mut by_name: HashMap<&str, Vec<&RustdocItem>> = HashMap::new();
    for item in &inventory.items {
        by_name.entry(item.name.as_str()).or_default().push(item);
    }

    let mut map = HashMap::new();
    for (id, summary) in &inventory.krate.paths {
        let path = summary.path.join("::");
        if inventory.items.iter().any(|item| item.path == path) {
            continue;
        }
        let Some(item) = inventory.krate.index.get(id) else {
            continue;
        };
        let Some(name) = item.name.as_deref() else {
            continue;
        };
        let Some(candidates) = by_name.get(name) else {
            continue;
        };
        if let Some(public) = candidates.iter().find(|candidate| candidate.kind.is_type()) {
            map.insert(path, public.path.clone());
        }
    }
    map
}

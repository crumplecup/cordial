//! Name lookup inside one rustdoc crate, the way rustc resolves a path.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::OnceLock;

use rustdoc_types::{Crate, Id, Item, ItemEnum};
use tracing::instrument;

use crate::error::CordialResult;

/// One allowlisted crate's rustdoc JSON, walked on demand.
///
/// Nothing is indexed up front: [`CrateIndex::lookup`] follows the segments
/// of one path through modules, `pub use` re-exports and glob re-exports,
/// so the work is proportional to the names asked about.
#[derive(Debug, derive_getters::Getters)]
pub struct CrateIndex {
    /// The crate's name as it appears as the first path segment.
    name: String,
    #[getter(skip)]
    krate: Crate,
    /// Defining path (`chrono::offset::utc::Utc`) to item, built on first use.
    #[getter(skip)]
    defining_paths: OnceLock<HashMap<String, Id>>,
}

/// Outcome of looking a path up in one crate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lookup {
    /// Exactly one item answers to the path.
    Found(Id),
    /// Nothing answers to the path.
    Missing,
    /// Several distinct items answer to the path.
    Ambiguous(Vec<Id>),
}

impl CrateIndex {
    /// Wrap a parsed rustdoc crate under `name`.
    #[instrument(level = "debug", skip(krate), fields(name = name))]
    pub fn new(name: &str, krate: Crate) -> Self {
        Self {
            name: name.to_string(),
            krate,
            defining_paths: OnceLock::new(),
        }
    }

    /// Read rustdoc JSON from `json_path` as the crate `name`.
    #[instrument(level = "debug", err(level = "warn"))]
    pub fn load(name: &str, json_path: &Path) -> CordialResult<Self> {
        let content = std::fs::read_to_string(json_path)?;
        Ok(Self::new(name, serde_json::from_str(&content)?))
    }

    /// The item with `id`, if this crate indexes it.
    #[instrument(level = "trace", skip(self, id))]
    pub fn item(&self, id: &Id) -> Option<&Item> {
        self.krate.index.get(id)
    }

    /// Defining path of `id` as `(crate_id, segments)`. `crate_id` is 0 for
    /// this crate.
    #[instrument(level = "trace", skip(self, id))]
    pub fn path_of(&self, id: &Id) -> Option<(u32, &[String])> {
        let summary = self.krate.paths.get(id)?;
        Some((summary.crate_id, summary.path.as_slice()))
    }

    /// The item defined at `path` (`chrono::offset::utc::Utc`), by rustdoc's
    /// own record of where each item is defined.
    ///
    /// This is how a canonical head is turned back into an item: the defining
    /// path often runs through a private module, and rustdoc strips private
    /// modules from the tree, so walking the tree by that path cannot find it.
    #[instrument(level = "trace", skip(self, path))]
    pub fn item_defined_at(&self, path: &str) -> Option<Id> {
        self.defining_paths
            .get_or_init(|| {
                self.krate
                    .paths
                    .iter()
                    .filter(|(_, summary)| summary.crate_id == 0)
                    .map(|(id, summary)| (summary.path.join("::"), *id))
                    .collect()
            })
            .get(path)
            .copied()
    }

    /// Ids of the traits implemented directly on the struct, enum or union
    /// `id`. Inherent impls are skipped. Blanket impls (`impl<T> Trait for T`)
    /// are not attached to the type in rustdoc JSON, so they are not seen.
    #[instrument(level = "trace", skip(self, id))]
    pub fn implemented_trait_ids(&self, id: &Id) -> Vec<Id> {
        let impls = match self.item(id).map(|item| &item.inner) {
            Some(ItemEnum::Struct(found)) => &found.impls,
            Some(ItemEnum::Enum(found)) => &found.impls,
            Some(ItemEnum::Union(found)) => &found.impls,
            _ => return Vec::new(),
        };
        impls
            .iter()
            .filter_map(|impl_id| match self.item(impl_id).map(|item| &item.inner) {
                Some(ItemEnum::Impl(found)) if !found.is_negative => {
                    found.trait_.as_ref().map(|path| path.id)
                }
                _ => None,
            })
            .collect()
    }

    /// Resolve `segments` (without the leading crate name) from the crate
    /// root. An empty slice resolves to the root module itself.
    #[instrument(level = "debug", skip(self))]
    pub fn lookup(&self, segments: &[&str]) -> Lookup {
        let mut current = vec![self.krate.root];
        for segment in segments {
            let mut next = Vec::new();
            for module in &current {
                let mut visited = HashSet::new();
                for found in self.children_named(*module, segment, &mut visited) {
                    if !next.contains(&found) {
                        next.push(found);
                    }
                }
            }
            current = next;
            if current.is_empty() {
                return Lookup::Missing;
            }
        }
        match current.as_slice() {
            [only] => Lookup::Found(*only),
            _ => Lookup::Ambiguous(current),
        }
    }

    /// Items named `name` directly in `module`, following `pub use` and glob
    /// re-exports. `visited` stops cycles and diamond re-exports.
    #[instrument(level = "debug", skip(self, module, visited))]
    fn children_named(&self, module: Id, name: &str, visited: &mut HashSet<Id>) -> Vec<Id> {
        let Some(ItemEnum::Module(m)) = self.item(&module).map(|item| &item.inner) else {
            return Vec::new();
        };
        if !visited.insert(module) {
            return Vec::new();
        }
        let mut found = Vec::new();
        for child_id in &m.items {
            let Some(child) = self.item(child_id) else {
                continue;
            };
            match &child.inner {
                ItemEnum::Use(import) if import.is_glob => {
                    if let Some(target) = import.id {
                        found.extend(self.children_named(target, name, visited));
                    }
                }
                ItemEnum::Use(import) => {
                    if import.name == name
                        && let Some(target) = import.id
                    {
                        found.push(target);
                    }
                }
                _ => {
                    if child.name.as_deref() == Some(name) {
                        found.push(*child_id);
                    }
                }
            }
        }
        found
    }
}

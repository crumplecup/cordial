//! Name lookup inside one rustdoc crate, the way rustc resolves a path.

use std::collections::HashSet;

use std::path::Path;

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

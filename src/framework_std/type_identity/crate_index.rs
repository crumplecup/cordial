//! Name lookup inside one rustdoc crate, the way rustc resolves a path.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::OnceLock;

use rustdoc_types::{
    Crate, GenericBound, GenericParamDefKind, Generics, Id, Item, ItemEnum, TraitBoundModifier,
    Type, WherePredicate,
};
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

/// One trait bound a type declares on one of its parameters, exactly as the
/// type's own definition writes it (`struct DateTime<Tz: TimeZone>`).
#[derive(Debug, Clone, PartialEq, Eq, derive_new::new, derive_getters::Getters)]
pub struct RawBound {
    /// The parameter's name (`Tz`, `K`).
    param: String,
    /// The parameter's position among the type's type and const parameters.
    #[getter(copy)]
    index: usize,
    /// The bounding trait, in the crate that declares the type.
    #[getter(copy)]
    trait_id: Id,
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

    /// The bounds the struct, enum or union `id` declares on its own type
    /// parameters, from inline bounds (`<K: Hash>`) and `where` clauses alike.
    ///
    /// These come from the type's definition, so any third-party generic
    /// carries them without help from the registry. Bounds that sit on impl
    /// blocks rather than on the type (as `HashMap`'s do) are not here: the
    /// type itself does not declare them. `?Sized` relaxations and lifetime
    /// bounds are skipped.
    #[instrument(level = "trace", skip(self, id))]
    pub fn declared_bounds(&self, id: &Id) -> Vec<RawBound> {
        let generics = match self.item(id).map(|item| &item.inner) {
            Some(ItemEnum::Struct(found)) => &found.generics,
            Some(ItemEnum::Enum(found)) => &found.generics,
            Some(ItemEnum::Union(found)) => &found.generics,
            _ => return Vec::new(),
        };
        raw_bounds(generics)
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

/// The declared bounds in one set of generics, positioned among the type and
/// const parameters (lifetimes do not appear in a type's argument list).
#[instrument(level = "trace", skip(generics))]
fn raw_bounds(generics: &Generics) -> Vec<RawBound> {
    let params: Vec<(&str, &[GenericBound])> = generics
        .params
        .iter()
        .filter_map(|param| match &param.kind {
            GenericParamDefKind::Type {
                bounds,
                is_synthetic: false,
                ..
            } => Some((param.name.as_str(), bounds.as_slice())),
            GenericParamDefKind::Const { .. } => Some((param.name.as_str(), &[][..])),
            _ => None,
        })
        .collect();
    let mut found = Vec::new();
    for (index, (name, bounds)) in params.iter().enumerate() {
        found.extend(trait_ids(bounds).map(|id| RawBound::new(name.to_string(), index, id)));
    }
    for predicate in &generics.where_predicates {
        let WherePredicate::BoundPredicate {
            type_: Type::Generic(name),
            bounds,
            ..
        } = predicate
        else {
            continue;
        };
        if let Some(index) = params.iter().position(|(param, _)| param == name) {
            found.extend(trait_ids(bounds).map(|id| RawBound::new(name.clone(), index, id)));
        }
    }
    found
}

/// The trait ids among `bounds`, without `?Trait` relaxations and lifetimes.
#[instrument(level = "debug", skip(bounds))]
fn trait_ids(bounds: &[GenericBound]) -> impl Iterator<Item = Id> + '_ {
    bounds.iter().filter_map(|bound| match bound {
        GenericBound::TraitBound {
            trait_,
            modifier: TraitBoundModifier::None,
            ..
        } => Some(trait_.id),
        _ => None,
    })
}

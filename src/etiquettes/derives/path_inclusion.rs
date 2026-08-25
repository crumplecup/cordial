//! Workspace-wide `#[path = "..."]` module-splice graph.
//!
//! **What.** Some crates reuse another crate's source file verbatim via
//! `#[path = "../other_crate/src/foo.rs"] mod foo;` instead of an ordinary
//! Cargo dependency -- real precedent: `amenable_verus` splices in eight
//! of `amenable_core`'s files this way, because the real `verus` binary
//! (`verus --crate-type=lib`) compiles one bare file tree and never
//! resolves an ordinary Cargo dependency, so it can't pull in
//! `amenable_core` (or `derive_new`/`derive_getters`/etc.) the normal
//! way. A spliced file compiles under two different dependency universes
//! at once: normally, as part of its owning crate, and again as raw
//! source inside whatever crate splices it in.
//!
//! **Why.** Recommending `#[derive(derive_new::new)]` for a type that
//! lives in a spliced file is only a real recommendation if *every* crate
//! that compiles that file actually has `derive_new` available. Checked
//! the hard way once already: adding a derive to `amenable_core::
//! provenance::MetadataEntry` (spliced into `amenable_verus`) compiled
//! clean for `amenable_core` alone but broke `cargo check --workspace
//! --all-features` with "private field, not a method" -- `amenable_verus`
//! silently got the un-expanded, pre-derive version of the same source.
//!
//! **How.** [`workspace_path_inclusions`] walks every workspace member's
//! source tree for `#[path]`-attributed `mod` items (resolving each
//! target relative to the file that names it, matching how `rustc`
//! itself resolves `#[path]`), and separately reads each member's real
//! dependency list via `cargo_metadata` (the same mechanism `antipatterns
//! ::version_in_member` already uses for its own workspace-wide check).
//! [`PathInclusionFacts::blocking_consumer`] answers, for one file and one
//! needed dependency, whether some *other* crate splices that file in
//! without the dependency -- the signal the derives scan needs to skip a
//! recommendation that isn't actually satisfiable everywhere the file
//! compiles.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use syn::Item;
use tracing::instrument;

/// Workspace-wide `#[path]`-splice graph plus each crate's real
/// dependency set.
#[derive(Debug, Default, Clone)]
pub struct PathInclusionFacts {
    /// Canonical spliced-file path -> crate names that splice it in.
    included_by: HashMap<PathBuf, Vec<String>>,
    /// Crate name -> normalized (`-` -> `_`) dependency names.
    crate_dependencies: HashMap<String, HashSet<String>>,
    /// Canonical crate root -> crate name.
    crate_name_by_root: HashMap<PathBuf, String>,
}

impl PathInclusionFacts {
    /// Name of a crate that splices `file` in via `#[path]` but lacks
    /// `needed_dep`, if any -- excluding whichever crate natively owns
    /// `owning_crate_root` (splicing a file into the crate that already
    /// natively contains it isn't a real conflict).
    #[instrument(level = "trace", skip(self))]
    pub fn blocking_consumer(
        &self,
        file: &Path,
        owning_crate_root: &Path,
        needed_dep: &str,
    ) -> Option<&str> {
        let file = canonical_or(file);
        let owning_name = self.crate_name_by_root.get(&canonical_or(owning_crate_root));
        let consumers = self.included_by.get(&file)?;
        let needed_dep = normalize(needed_dep);
        consumers
            .iter()
            .find(|consumer| {
                Some(consumer.as_str()) != owning_name.map(String::as_str)
                    && !self
                        .crate_dependencies
                        .get(consumer.as_str())
                        .is_some_and(|deps| deps.contains(&needed_dep))
            })
            .map(String::as_str)
    }
}

type FactsCache = Mutex<Option<(PathBuf, PathInclusionFacts)>>;
static FACTS_CACHE: FactsCache = Mutex::new(None);

/// Cached, workspace-wide `#[path]`-splice + dependency facts. Cheap to
/// call repeatedly (once per crate scanned in one session): computed once
/// per `workspace_root`, matching `antipatterns::version_in_member`'s own
/// cache shape.
#[instrument(level = "debug")]
pub fn workspace_path_inclusions(workspace_root: &Path) -> PathInclusionFacts {
    let cache_key = canonical_or(workspace_root);
    if let Ok(cache) = FACTS_CACHE.lock()
        && let Some((key, facts)) = cache.as_ref()
        && *key == cache_key
    {
        return facts.clone();
    }

    let facts = compute_path_inclusions(workspace_root);
    if let Ok(mut cache) = FACTS_CACHE.lock() {
        *cache = Some((cache_key, facts.clone()));
    }
    facts
}

#[instrument(level = "debug")]
fn compute_path_inclusions(workspace_root: &Path) -> PathInclusionFacts {
    let Ok(meta) = cargo_metadata::MetadataCommand::new()
        .current_dir(workspace_root)
        .exec()
    else {
        return PathInclusionFacts::default();
    };

    let mut crate_dependencies = HashMap::new();
    let mut crate_name_by_root = HashMap::new();
    let mut crate_roots = Vec::new();

    for package_id in &meta.workspace_members {
        let Some(package) = meta
            .packages
            .iter()
            .find(|candidate| &candidate.id == package_id)
        else {
            continue;
        };
        let Some(crate_root) = Path::new(package.manifest_path.as_str()).parent() else {
            continue;
        };
        let crate_root = crate_root.to_path_buf();
        let deps = package
            .dependencies
            .iter()
            .map(|dep| normalize(&dep.name))
            .collect::<HashSet<_>>();
        crate_dependencies.insert(package.name.to_string(), deps);
        crate_name_by_root.insert(canonical_or(&crate_root), package.name.to_string());
        crate_roots.push((package.name.to_string(), crate_root));
    }

    let included_by = discover_splices(&crate_roots);

    PathInclusionFacts {
        included_by,
        crate_dependencies,
        crate_name_by_root,
    }
}

#[instrument(level = "debug", skip(crate_roots))]
fn discover_splices(crate_roots: &[(String, PathBuf)]) -> HashMap<PathBuf, Vec<String>> {
    let mut included_by: HashMap<PathBuf, Vec<String>> = HashMap::new();
    for (crate_name, crate_root) in crate_roots {
        let src_root = crate_root.join("src");
        if !src_root.is_dir() {
            continue;
        }
        for entry in walkdir::WalkDir::new(&src_root)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
        {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "rs") {
                continue;
            }
            let Ok(source) = std::fs::read_to_string(path) else {
                continue;
            };
            let Ok(syntax) = syn::parse_file(&source) else {
                continue;
            };
            let Some(file_dir) = path.parent() else {
                continue;
            };
            for target in path_attr_targets(&syntax.items) {
                let resolved = canonical_or(&file_dir.join(&target));
                included_by
                    .entry(resolved)
                    .or_default()
                    .push(crate_name.clone());
            }
        }
    }
    included_by
}

/// Every `#[path = "..."]` target on a `mod` item, recursing into nested
/// inline modules (an inline `mod` with its own `#[path]`-attributed
/// children is legal, if unusual).
#[instrument(level = "trace", skip(items))]
fn path_attr_targets(items: &[Item]) -> Vec<String> {
    let mut targets = Vec::new();
    for item in items {
        if let Item::Mod(item_mod) = item {
            if let Some(target) = path_attr_value(&item_mod.attrs) {
                targets.push(target);
            }
            if let Some((_, nested)) = &item_mod.content {
                targets.extend(path_attr_targets(nested));
            }
        }
    }
    targets
}

#[instrument(level = "trace", skip(attrs))]
fn path_attr_value(attrs: &[syn::Attribute]) -> Option<String> {
    attrs.iter().find_map(|attr| {
        if !attr.path().is_ident("path") {
            return None;
        }
        let syn::Meta::NameValue(name_value) = &attr.meta else {
            return None;
        };
        let syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(lit_str),
            ..
        }) = &name_value.value
        else {
            return None;
        };
        Some(lit_str.value())
    })
}

#[instrument(level = "trace")]
fn normalize(name: &str) -> String {
    name.replace('-', "_")
}

#[instrument(level = "trace")]
fn canonical_or(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

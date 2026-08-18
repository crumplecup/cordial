//! Match std inventory paths against impl-crate trait impl registrations.

use std::collections::HashSet;
use std::path::Path;

use rustdoc_types::{Crate, ItemEnum, Type};
use tracing::instrument;

use crate::error::CordialResult;
use crate::rustdoc::{RustdocInventory, parse_rustdoc_json};

/// Collect canonical type paths that have `impl {trait_name} for T` in a crate's rustdoc JSON.
#[instrument(level = "debug", err(level = "warn"))]
pub fn collect_trait_impl_paths_from_json(
    json_path: &Path,
    crate_name: &str,
    trait_name: &str,
) -> CordialResult<HashSet<String>> {
    let inventory = parse_rustdoc_json(json_path, crate_name)?;
    Ok(collect_trait_impl_paths(&inventory, trait_name))
}

/// Collect trait impl type paths from parsed rustdoc inventory.
#[instrument(level = "debug", skip(inventory))]
pub fn collect_trait_impl_paths(inventory: &RustdocInventory, trait_name: &str) -> HashSet<String> {
    let mut paths = HashSet::new();
    for item in inventory.krate.index.values() {
        let ItemEnum::Impl(impl_item) = &item.inner else {
            continue;
        };
        let Some(trait_) = &impl_item.trait_ else {
            continue;
        };
        if trait_.path.rsplit("::").next() != Some(trait_name) {
            continue;
        }
        if let Some(path) = impl_type_path(&inventory.krate, &impl_item.for_) {
            paths.insert(path);
        }
    }
    paths
}

fn impl_type_path(krate: &Crate, ty: &Type) -> Option<String> {
    match ty {
        Type::ResolvedPath(path) => {
            let summary = krate.paths.get(&path.id)?;
            Some(summary.path.join("::"))
        }
        Type::Primitive(name) => Some(name.clone()),
        _ => None,
    }
}

/// Strip generic/lifetime arguments from a type path string for inventory matching.
#[instrument(level = "debug")]
pub fn type_path_without_generics(type_str: &str) -> String {
    let mut depth = 0i32;
    for (index, ch) in type_str.char_indices() {
        match ch {
            '<' if depth == 0 => return type_str[..index].trim().to_string(),
            '<' => depth += 1,
            '>' => depth -= 1,
            _ => {}
        }
    }
    type_str.trim().to_string()
}

/// Whether `type_path` from a std inventory row has a matching trait impl path.
#[instrument(level = "debug")]
pub fn type_has_trait_impl(impl_paths: &HashSet<String>, type_path: &str) -> bool {
    if impl_paths.contains(type_path) {
        return true;
    }
    let type_tail = path_without_crate_root(type_path);
    let type_shape = compound_primitive_shape(type_path);
    impl_paths.iter().any(|impl_path| {
        let impl_base = type_path_without_generics(impl_path);
        if impl_base == type_path {
            return true;
        }
        if !impl_base.contains("::") {
            let bare = type_path.rsplit("::").next();
            let impl_shape = compound_primitive_shape(&impl_base);
            return bare == Some(impl_base.as_str())
                || impl_shape == bare
                || (type_shape.is_some() && type_shape == impl_shape);
        }
        path_without_crate_root(&impl_base) == type_tail
    })
}

fn compound_primitive_shape(text: &str) -> Option<&'static str> {
    let text = text.trim();
    if text == "()" {
        return Some("unit");
    }
    if let Some(inner) = text
        .strip_prefix('(')
        .and_then(|rest| rest.strip_suffix(')'))
        && inner.contains(',')
    {
        return Some("tuple");
    }
    if let Some(inner) = text
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
    {
        return Some(if inner.contains(';') {
            "array"
        } else {
            "slice"
        });
    }
    if text.starts_with("fn(") || text.starts_with("fn (") {
        return Some("fn");
    }
    if text.starts_with("*const ") || text.starts_with("*mut ") {
        return Some("pointer");
    }
    if text.starts_with('&') {
        return Some("reference");
    }
    None
}

fn path_without_crate_root(path: &str) -> &str {
    path.split_once("::").map_or(path, |(_root, rest)| rest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_has_trait_impl_matches_bare_and_qualified_paths() {
        let impls: HashSet<String> = ["i32", "std::collections::HashMap"]
            .into_iter()
            .map(str::to_string)
            .collect();
        assert!(type_has_trait_impl(&impls, "i32"));
        assert!(type_has_trait_impl(&impls, "std::primitive::i32"));
        assert!(type_has_trait_impl(&impls, "std::collections::HashMap"));
        assert!(!type_has_trait_impl(&impls, "std::vec::Vec"));
    }

    #[test]
    fn type_has_trait_impl_does_not_cross_match_unrelated_types_sharing_a_bare_name() {
        let impls: HashSet<String> = HashSet::from(["core::fmt::Error".to_string()]);
        assert!(type_has_trait_impl(&impls, "core::fmt::Error"));
        assert!(!type_has_trait_impl(&impls, "std::io::Error"));
    }

    #[test]
    fn type_has_trait_impl_matches_across_a_std_core_reexport() {
        let impls: HashSet<String> = HashSet::from(["core::fmt::Alignment".to_string()]);
        assert!(type_has_trait_impl(&impls, "std::fmt::Alignment"));
    }

    #[test]
    fn type_has_trait_impl_matches_representative_generic_instantiation() {
        let impls: HashSet<String> = HashSet::from(["std::sync::mpsc::Sender<i32>".to_string()]);
        assert!(type_has_trait_impl(&impls, "std::sync::mpsc::Sender"));
    }
}

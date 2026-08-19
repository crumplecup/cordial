//! Walk rustdoc [`Type`] and generics nodes for public signature dependencies.

use std::collections::HashSet;

use tracing::{debug, instrument};

use super::generics::{
    collect_items_from_generic_args, collect_items_from_generic_bound,
    collect_items_from_generic_param_defs, collect_items_from_poly_trait, type_kind_name,
};
use super::item::build_inventory_item;
use super::{ExtractedItem, path_matches_scope};

#[instrument(level = "debug", skip(krate, ty, seen, discovered))]
pub(super) fn collect_items_from_type(
    krate: &rustdoc_types::Crate,
    ty: &rustdoc_types::Type,
    own_crate_key: &str,
    prefix_match: bool,
    seen: &mut HashSet<String>,
    discovered: &mut Vec<ExtractedItem>,
) {
    match ty {
        rustdoc_types::Type::ResolvedPath(resolved) => {
            if let Some(summary) = krate.paths.get(&resolved.id) {
                let path = summary.path.join("::");
                if !path_matches_scope(&summary.path, own_crate_key, prefix_match)
                    && !path.starts_with("std::")
                    && !path.starts_with("core::")
                    && !path.starts_with("alloc::")
                {
                    if seen.insert(path) {
                        debug!(
                            discovered_path = %summary.path.join("::"),
                            "discovered signature dependency from resolved type"
                        );
                        if let Some(item) = build_inventory_item(krate, &resolved.id, summary) {
                            discovered.push(item);
                        }
                    }
                } else {
                    debug!(
                        candidate_path = %summary.path.join("::"),
                        in_scope = path_matches_scope(&summary.path, own_crate_key, prefix_match),
                        std_like = path.starts_with("std::")
                            || path.starts_with("core::")
                            || path.starts_with("alloc::"),
                        "skipping resolved type during signature dependency discovery"
                    );
                }
            }
            if let Some(args) = &resolved.args {
                collect_items_from_generic_args(
                    krate,
                    args,
                    own_crate_key,
                    prefix_match,
                    seen,
                    discovered,
                );
            }
        }
        rustdoc_types::Type::BorrowedRef { type_, .. }
        | rustdoc_types::Type::RawPointer { type_, .. }
        | rustdoc_types::Type::Slice(type_)
        | rustdoc_types::Type::Array { type_, .. } => {
            collect_items_from_type(krate, type_, own_crate_key, prefix_match, seen, discovered)
        }
        rustdoc_types::Type::Tuple(items) => {
            for item in items {
                collect_items_from_type(krate, item, own_crate_key, prefix_match, seen, discovered);
            }
        }
        rustdoc_types::Type::FunctionPointer(function_pointer) => {
            for (_, input) in &function_pointer.sig.inputs {
                collect_items_from_type(
                    krate,
                    input,
                    own_crate_key,
                    prefix_match,
                    seen,
                    discovered,
                );
            }
            if let Some(output) = &function_pointer.sig.output {
                collect_items_from_type(
                    krate,
                    output,
                    own_crate_key,
                    prefix_match,
                    seen,
                    discovered,
                );
            }
            collect_items_from_generic_param_defs(
                krate,
                &function_pointer.generic_params,
                own_crate_key,
                prefix_match,
                seen,
                discovered,
            );
        }
        rustdoc_types::Type::DynTrait(dyn_trait) => {
            for bound in &dyn_trait.traits {
                collect_items_from_poly_trait(
                    krate,
                    bound,
                    own_crate_key,
                    prefix_match,
                    seen,
                    discovered,
                );
            }
        }
        rustdoc_types::Type::ImplTrait(bounds) => {
            for bound in bounds {
                collect_items_from_generic_bound(
                    krate,
                    bound,
                    own_crate_key,
                    prefix_match,
                    seen,
                    discovered,
                );
            }
        }
        rustdoc_types::Type::QualifiedPath {
            self_type,
            trait_,
            args,
            ..
        } => {
            collect_items_from_type(
                krate,
                self_type,
                own_crate_key,
                prefix_match,
                seen,
                discovered,
            );
            if let Some(trait_) = trait_ {
                collect_items_from_path(
                    krate,
                    trait_,
                    own_crate_key,
                    prefix_match,
                    seen,
                    discovered,
                );
            }
            if let Some(args) = args {
                collect_items_from_generic_args(
                    krate,
                    args,
                    own_crate_key,
                    prefix_match,
                    seen,
                    discovered,
                );
            }
        }
        rustdoc_types::Type::Primitive(_)
        | rustdoc_types::Type::Generic(_)
        | rustdoc_types::Type::Infer => {}
        _ => {}
    }
}
#[instrument(level = "debug", skip(krate, path, seen, discovered))]
pub(super) fn collect_items_from_path(
    krate: &rustdoc_types::Crate,
    path: &rustdoc_types::Path,
    own_crate_key: &str,
    prefix_match: bool,
    seen: &mut HashSet<String>,
    discovered: &mut Vec<ExtractedItem>,
) {
    if let Some(summary) = krate.paths.get(&path.id) {
        let path_str = summary.path.join("::");
        if !path_matches_scope(&summary.path, own_crate_key, prefix_match)
            && !path_str.starts_with("std::")
            && !path_str.starts_with("core::")
            && !path_str.starts_with("alloc::")
        {
            if seen.insert(path_str) {
                debug!(
                    discovered_path = %summary.path.join("::"),
                    "discovered signature dependency from path"
                );
                if let Some(item) = build_inventory_item(krate, &path.id, summary) {
                    discovered.push(item);
                }
            }
        } else {
            debug!(
                candidate_path = %summary.path.join("::"),
                in_scope = path_matches_scope(&summary.path, own_crate_key, prefix_match),
                std_like = path_str.starts_with("std::")
                    || path_str.starts_with("core::")
                    || path_str.starts_with("alloc::"),
                "skipping path during signature dependency discovery"
            );
        }
    }
    if let Some(args) = &path.args {
        collect_items_from_generic_args(krate, args, own_crate_key, prefix_match, seen, discovered);
    }
}

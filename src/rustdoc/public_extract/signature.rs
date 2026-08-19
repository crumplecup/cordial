//! Walk rustdoc types in public signatures to discover dependency items.

use std::collections::HashSet;

use tracing::{debug, instrument};

use super::{ExtractedItem, item_is_public, path_matches_scope};

#[instrument(level = "debug", skip(krate, existing_paths))]
pub(super) fn collect_public_signature_dependency_items(
    krate: &rustdoc_types::Crate,
    own_crate_key: &str,
    prefix_match: bool,
    existing_paths: &HashSet<String>,
) -> Vec<ExtractedItem> {
    let mut discovered = Vec::new();
    let mut seen = existing_paths.clone();

    for (id, item) in &krate.index {
        match &item.inner {
            rustdoc_types::ItemEnum::Function(function)
                if item_is_public(item)
                    && krate.paths.get(id).is_some_and(|summary| {
                        path_matches_scope(&summary.path, own_crate_key, prefix_match)
                    }) =>
            {
                collect_items_from_function_signature(
                    krate,
                    function,
                    own_crate_key,
                    prefix_match,
                    &mut seen,
                    &mut discovered,
                );
            }
            rustdoc_types::ItemEnum::Trait(trait_item)
                if item_is_public(item)
                    && krate.paths.get(id).is_some_and(|summary| {
                        path_matches_scope(&summary.path, own_crate_key, prefix_match)
                    }) =>
            {
                for child_id in &trait_item.items {
                    let Some(child) = krate.index.get(child_id) else {
                        continue;
                    };
                    if !item_is_public(child) {
                        continue;
                    }
                    if let rustdoc_types::ItemEnum::Function(function) = &child.inner {
                        collect_items_from_function_signature(
                            krate,
                            function,
                            own_crate_key,
                            prefix_match,
                            &mut seen,
                            &mut discovered,
                        );
                    }
                }
            }
            rustdoc_types::ItemEnum::Impl(impl_item)
                if impl_item.trait_.is_none()
                    && inherent_impl_targets_scope(
                        krate,
                        impl_item,
                        own_crate_key,
                        prefix_match,
                    ) =>
            {
                for child_id in &impl_item.items {
                    let Some(child) = krate.index.get(child_id) else {
                        continue;
                    };
                    if !item_is_public(child) {
                        continue;
                    }
                    if let rustdoc_types::ItemEnum::Function(function) = &child.inner {
                        collect_items_from_function_signature(
                            krate,
                            function,
                            own_crate_key,
                            prefix_match,
                            &mut seen,
                            &mut discovered,
                        );
                    }
                }
            }
            _ => {}
        }
    }

    debug!(
        discovered_count = discovered.len(),
        "collected public signature dependency items"
    );

    discovered
}

#[instrument(level = "debug", skip(krate, impl_item))]
fn inherent_impl_targets_scope(
    krate: &rustdoc_types::Crate,
    impl_item: &rustdoc_types::Impl,
    own_crate_key: &str,
    prefix_match: bool,
) -> bool {
    let rustdoc_types::Type::ResolvedPath(resolved) = &impl_item.for_ else {
        return false;
    };
    krate
        .paths
        .get(&resolved.id)
        .is_some_and(|summary| path_matches_scope(&summary.path, own_crate_key, prefix_match))
}

#[instrument(level = "debug", skip(krate, function, seen, discovered))]
fn collect_items_from_function_signature(
    krate: &rustdoc_types::Crate,
    function: &rustdoc_types::Function,
    own_crate_key: &str,
    prefix_match: bool,
    seen: &mut HashSet<String>,
    discovered: &mut Vec<ExtractedItem>,
) {
    for (_, input) in &function.sig.inputs {
        super::type_walk::collect_items_from_type(
            krate,
            input,
            own_crate_key,
            prefix_match,
            seen,
            discovered,
        );
    }
    if let Some(output) = &function.sig.output {
        super::type_walk::collect_items_from_type(
            krate,
            output,
            own_crate_key,
            prefix_match,
            seen,
            discovered,
        );
    }
    super::generics::collect_items_from_generics(
        krate,
        &function.generics,
        own_crate_key,
        prefix_match,
        seen,
        discovered,
    );

    debug!(
        discovered_count = discovered.len(),
        "processed function signature for dependency discovery"
    );
}

//! Visibility names from the parsed Verus dialect body.
use super::{Items, VerusBlock};
use tracing::instrument;

impl VerusBlock {
    #[tracing::instrument(level = "trace", skip(self))]
    pub(in crate::verus_ir) fn visible_names(&self) -> (usize, usize) {
        count_item_names(&self.items)
    }
}

/// Count real, externally-visible names among the items inside one
/// `verus! { .. }` invocation's own token stream: `pub` items count
/// toward a crate's externally reachable API surface, `pub(crate)`/
/// `pub(in ..)` toward its crate-internal one, `pub(super)` toward
/// neither -- the exact question `crate::etiquettes::visibility`'s
/// thin-module check asks of ordinary Rust items, which `syn::visit::
/// Visit` can never ask of anything inside a macro's own token stream
/// (and couldn't parse as plain Rust even if it tried to -- Verus's
/// `spec fn`/`open`/`closed`/`requires`/`ensures` aren't valid `syn`
/// grammar). Returns `(pub_names, pub_crate_names)`; a block that fails
/// to parse (a real syntax error, or a `verus_syn` version mismatch
/// against the toolchain that generated the source) contributes
/// `(0, 0)` -- the same best-effort posture as every other `verus_ir`
/// entry point.
#[instrument(level = "debug", skip(tokens))]
pub(crate) fn count_verus_item_names(tokens: proc_macro2::TokenStream) -> (usize, usize) {
    let Ok(parsed) = verus_syn::parse2::<Items>(tokens) else {
        return (0, 0);
    };
    count_item_names(&parsed.items)
}

#[tracing::instrument(level = "debug", skip(items))]
fn count_item_names(items: &[verus_syn::Item]) -> (usize, usize) {
    let mut leaf_pub = 0usize;
    let mut leaf_crate = 0usize;
    for item in items {
        let Some((vis, n)) = verus_item_vis_and_name_count(item) else {
            continue;
        };
        if n == 0 {
            continue;
        }
        match vis {
            verus_syn::Visibility::Public(_) => {
                leaf_pub += n;
                leaf_crate += n;
            }
            verus_syn::Visibility::Restricted(restricted) => {
                if !restricted.path.is_ident("super") {
                    leaf_crate += n;
                }
            }
            verus_syn::Visibility::Inherited => {}
        }
    }
    (leaf_pub, leaf_crate)
}

/// Mirrors `crate::etiquettes::visibility::scan::vis::item_vis`/
/// `leaf_name_count`, ported to `verus_syn`'s own (structurally
/// identical, but type-incompatible) `Item`/`Visibility` -- the same
/// local-duplication convention this codebase already uses for small
/// per-etiquette AST helpers (e.g. its half-dozen local `type_label`
/// copies), since the two `Item` enums can't share one generic
/// implementation without a wrapper trait neither crate provides.
#[instrument(level = "debug", skip(item))]
fn verus_item_vis_and_name_count(
    item: &verus_syn::Item,
) -> Option<(&verus_syn::Visibility, usize)> {
    match item {
        verus_syn::Item::Const(item) => Some((&item.vis, 1)),
        verus_syn::Item::Enum(item) => Some((&item.vis, 1)),
        verus_syn::Item::Fn(item) => Some((&item.vis, 1)),
        verus_syn::Item::Static(item) => Some((&item.vis, 1)),
        verus_syn::Item::Struct(item) => Some((&item.vis, 1)),
        verus_syn::Item::Trait(item) => Some((&item.vis, 1)),
        verus_syn::Item::TraitAlias(item) => Some((&item.vis, 1)),
        verus_syn::Item::Type(item) => Some((&item.vis, 1)),
        verus_syn::Item::Union(item) => Some((&item.vis, 1)),
        verus_syn::Item::Use(item) => Some((&item.vis, verus_use_name_count(&item.tree))),
        _ => None,
    }
}

#[instrument(level = "debug", skip(tree))]
fn verus_use_name_count(tree: &verus_syn::UseTree) -> usize {
    match tree {
        verus_syn::UseTree::Name(_) | verus_syn::UseTree::Rename(_) => 1,
        verus_syn::UseTree::Glob(_) => 0,
        verus_syn::UseTree::Path(path) => verus_use_name_count(&path.tree),
        verus_syn::UseTree::Group(group) => group.items.iter().map(verus_use_name_count).sum(),
    }
}

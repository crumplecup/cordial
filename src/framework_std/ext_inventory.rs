//! Load a third-party (`amenable_ext`) target crate's inventory from
//! shadow-dep rustdoc JSON — the `amenable_ext` counterpart of
//! [`super::inventory::load_merged_std_inventory`], which loads std/core/
//! alloc from the shared sysroot cache instead.
//!
//! `crate::cargo_rustdoc::build_shadow_dep_rustdoc` resolves its feature
//! set via a plain `cargo_metadata` read of `shadow_crate`'s own
//! `Cargo.toml` dependency edge on `upstream_crate` before falling back
//! to the `elicitation`-specific tracked-shadow-pair lookup — so once a
//! crate like `amenable_ext` declares a real (even optional) dependency
//! on `upstream_crate` (`jiff`, say), this just works: no
//! `elicitation`-style shadow-pair registration needed at all. The
//! "shadow" in these names is a legacy of their first caller, not a
//! real constraint on this use.

use std::path::Path;

use tracing::instrument;

use crate::cargo_rustdoc::build_shadow_dep_rustdoc;
use crate::error::CordialResult;
use crate::framework_std::StdInventoryItem;
use crate::framework_std::inventory::load_std_inventory_from_json;
use crate::store::StoreLayout;

/// Build (or reuse the cache for) `upstream_crate`'s rustdoc JSON via
/// `shadow_crate`'s own Cargo dependency on it, then load its public
/// inventory the same way `load_std_inventory_from_json` (private to
/// this module, no public path to link) loads a std-family source.
#[instrument(level = "info", skip(store), err(level = "warn"))]
pub fn load_ext_inventory_from_shadow_dep(
    project_root: &Path,
    store: &StoreLayout,
    shadow_crate: &str,
    upstream_crate: &str,
    force: bool,
) -> CordialResult<Vec<StdInventoryItem>> {
    build_shadow_dep_rustdoc(project_root, store, shadow_crate, upstream_crate, force)?;
    let json_path = store.shadow_dep_rustdoc_cache_path(shadow_crate, upstream_crate);
    load_std_inventory_from_json(&json_path, upstream_crate)
}

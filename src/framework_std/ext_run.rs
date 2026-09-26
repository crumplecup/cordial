//! Amenable ext (third-party crate) registry coverage orchestration —
//! the `amenable_ext` counterpart of [`super::amenable_run`], which
//! assesses `amenable_std` against the shared sysroot inventory instead.
//!
//! Unlike the std case, crate names here vary per target (`jiff` today,
//! `chrono` tomorrow), so they're explicit parameters to
//! [`assess_amenable_ext_coverage`] rather than fixed constants the way
//! `AMENABLE_IMPL_CRATE`/`AMENABLE_PATCH_SET` are for std.

use std::path::Path;

use tracing::instrument;

use crate::error::CordialResult;
use crate::framework_std::amenable::{AmenableStdReport, build_amenable_ext_report};
use crate::framework_std::amenable_run::{AmenableStdOptions, ensure_registry_dump_for_assessor};
use crate::framework_std::ext_inventory::load_ext_inventory_from_shadow_dep;
use crate::framework_std::proof_harness::collect_proof_chain_subjects;
use crate::framework_std::verifier_skip::load_verifier_skip_map;
use crate::session::SessionView;
use crate::store::StoreLayout;

/// The crate that both depends on `jiff` (the shadow crate
/// `build_shadow_dep_rustdoc` resolves its feature set from) and
/// provides jiff's `ExtType`/`ExtStandard<T>` impls (the impl crate) —
/// the same crate serves both roles here, unlike `amenable_std`'s own
/// `AMENABLE_IMPL_CRATE`, which has no shadow-crate concept at all
/// (its inventory comes from the sysroot cache, not a shadow-dep build).
pub const AMENABLE_EXT_IMPL_CRATE: &str = "amenable_ext";
/// The upstream target crate for the jiff coverage etiquette specifically.
pub const AMENABLE_EXT_JIFF_UPSTREAM_CRATE: &str = "jiff";
/// Patch set for jiff-specific verifier skip entries.
pub const AMENABLE_EXT_JIFF_PATCH_SET: &str = "amenable_ext_jiff";

/// Options for amenable ext registry coverage assessment. A separate
/// type from [`AmenableStdOptions`], not a reuse of it: this flow has a
/// third, independent cache-refresh axis (the shadow-dep rustdoc cache)
/// the std flow doesn't need — std's inventory comes from the shared
/// sysroot cache instead, which has its own separate freshness check.
#[derive(Debug, Clone, Copy, Default, derive_getters::Getters, derive_setters::Setters)]
#[setters(prefix = "with_")]
pub struct AmenableExtOptions {
    /// Whether nightly-only items are in scope.
    #[getter(copy)]
    include_nightly: bool,
    /// Re-run `amenable dump-registry` even when a cached dump exists.
    #[getter(copy)]
    refresh_registry: bool,
    /// Rebuild the upstream crate's rustdoc JSON even when its cache is fresh.
    #[getter(copy)]
    force_rustdoc: bool,
}

/// Assess amenable ext registry coverage for one upstream target crate,
/// using shadow-dep rustdoc inventory and the shared registry dump (the
/// same dump [`assess_amenable_std_coverage`](super::amenable_run::assess_amenable_std_coverage)
/// uses — one real `amenable dump-registry` run covers every backend
/// linked into that binary, std and ext target types alike).
#[instrument(level = "debug", skip(session, store, options), err(level = "warn"))]
pub fn assess_amenable_ext_coverage(
    session: &dyn SessionView,
    store: &StoreLayout,
    project_root: &Path,
    shadow_crate: &str,
    upstream_crate: &str,
    patch_set: &str,
    options: &AmenableExtOptions,
) -> CordialResult<AmenableStdReport> {
    let _ = session;
    let items = load_ext_inventory_from_shadow_dep(
        project_root,
        store,
        shadow_crate,
        upstream_crate,
        options.force_rustdoc(),
    )?;
    let registry_options =
        AmenableStdOptions::default().with_refresh_registry(options.refresh_registry());
    let registry = ensure_registry_dump_for_assessor(store, project_root, &registry_options)?;
    let skip_map = load_verifier_skip_map(store, patch_set);
    let proof_chain_subjects = collect_proof_chain_subjects(project_root)?;
    build_amenable_ext_report(
        upstream_crate,
        &items,
        shadow_crate,
        &registry,
        &skip_map,
        &proof_chain_subjects,
        options.include_nightly(),
    )
}

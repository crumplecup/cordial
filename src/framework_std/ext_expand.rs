//! Per-instantiation rows for an amenable-ext target.
//!
//! Builds the type resolver from the rustdoc JSON of the target crate and the
//! other crates its configuration allows, resolves the registry against it,
//! and expands each generic row into a parent and its instantiation rows. The
//! rustdoc and the configuration are the only inputs beyond the registry.

use std::collections::HashSet;
use std::path::Path;

use tracing::instrument;

use crate::cargo_rustdoc::{build_shadow_dep_rustdoc, member_dependency_package_name};
use crate::config::{AmenableExtConfig, AmenableExtTargetConfig};
use crate::error::CordialResult;
use crate::framework_std::instantiation::{
    AmenableRegistryEvidence, ExpectedInstantiations, InstantiationContext, expand_entries,
    expand_report,
};
use crate::framework_std::registry::RegistryDump;
use crate::framework_std::type_identity::{CrateIndex, ResolveCaps, RustdocTypeResolver};
use crate::framework_std::verifier_skip::VerifierSkipMap;
use crate::framework_std::{AmenableStdEntry, AmenableStdReport};
use crate::store::StoreLayout;

/// The crates the resolver may read for a target: the target itself first,
/// then its configured extras, each once.
#[instrument(level = "debug", skip(target))]
pub fn resolver_crates(
    upstream_crate: &str,
    target: Option<&AmenableExtTargetConfig>,
) -> Vec<String> {
    let mut crates = vec![upstream_crate.to_string()];
    for extra in target
        .map(|t| t.resolve_crates().as_slice())
        .unwrap_or_default()
    {
        if !crates.contains(extra) {
            crates.push(extra.clone());
        }
    }
    crates
}

/// Build (or reuse the cache for) each crate's rustdoc JSON through
/// `shadow_crate`'s dependency on it, and read them into one resolver.
#[instrument(level = "info", skip(store, config), err(level = "warn"))]
pub fn load_ext_resolver(
    project_root: &Path,
    store: &StoreLayout,
    shadow_crate: &str,
    crates: &[String],
    config: &AmenableExtConfig,
    force: bool,
) -> CordialResult<RustdocTypeResolver> {
    let mut indexes = Vec::with_capacity(crates.len());
    for krate in crates {
        // Cargo knows the package name (`chrono-tz`); rustdoc paths use the
        // crate name (`chrono_tz`). Configuration may spell either.
        let package = member_dependency_package_name(project_root, shadow_crate, krate)?;
        build_shadow_dep_rustdoc(project_root, store, shadow_crate, &package, force)?;
        let json = store.shadow_dep_rustdoc_cache_path(shadow_crate, &package);
        indexes.push(CrateIndex::load(&krate.replace('-', "_"), &json)?);
    }
    Ok(RustdocTypeResolver::new(
        indexes,
        ResolveCaps::new(config.alias_depth(), config.max_type_nodes()),
    ))
}

/// What the expansion reads besides the rows.
#[derive(derive_new::new)]
pub struct ExtExpansionInputs<'a> {
    resolver: &'a RustdocTypeResolver,
    registry: &'a RegistryDump,
    proof_chain_subjects: &'a HashSet<String>,
    skip_map: &'a VerifierSkipMap,
    config: &'a AmenableExtConfig,
    target: Option<&'a AmenableExtTargetConfig>,
}

/// Expand the generic rows among `entries` into parents and instantiations.
#[instrument(level = "debug", skip(entries, inputs), err(level = "warn"))]
pub fn expand_ext_entries(
    entries: &[AmenableStdEntry],
    inputs: &ExtExpansionInputs<'_>,
) -> CordialResult<Vec<AmenableStdEntry>> {
    let evidence = inputs.evidence();
    let expected = inputs.expected();
    let ctx = InstantiationContext::new(inputs.resolver, &evidence, inputs.skip_map, &expected);
    expand_entries(entries, &ctx)
}

/// The same expansion over a whole report, recounted.
#[instrument(level = "debug", skip(report, inputs), err(level = "warn"))]
pub fn expand_ext_report(
    report: &AmenableStdReport,
    inputs: &ExtExpansionInputs<'_>,
) -> CordialResult<AmenableStdReport> {
    let evidence = inputs.evidence();
    let expected = inputs.expected();
    let ctx = InstantiationContext::new(inputs.resolver, &evidence, inputs.skip_map, &expected);
    expand_report(report, &ctx)
}

impl ExtExpansionInputs<'_> {
    /// The registry, resolved against the resolver.
    #[instrument(level = "trace", skip(self))]
    fn evidence(&self) -> AmenableRegistryEvidence {
        AmenableRegistryEvidence::resolve(self.registry, self.proof_chain_subjects, self.resolver)
    }

    /// The configured instantiations and the derivation cap.
    #[instrument(level = "trace", skip(self))]
    fn expected(&self) -> ExpectedInstantiations {
        ExpectedInstantiations::new(
            self.target
                .map(|target| target.instantiations().clone())
                .unwrap_or_default(),
        )
        .with_derive_cap(self.config.derive_cap())
    }
}

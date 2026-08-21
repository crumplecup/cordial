//! Deduplicate etiquette hooks and resolve the active etiquette set.

use std::collections::{HashMap, HashSet};

use crate::enricher::{PathIndexEnricher, SynDocLinkEnricher};
use crate::error::{CordialError, CordialResult};
use crate::etiquette::Etiquette;
use crate::hooks::{Assessor, IrEnricher, Loader, Probe, Reporter, WorkspaceAssessor};
use crate::loader::{LoadView, SourceLoader};
use crate::plugin::{Plugin, etiquettes_from_plugins, selected_plugins};
#[cfg(feature = "rustdoc")]
use crate::{RustdocLoader, enricher::RustdocStructureEnricher};
use tracing::instrument;

use super::RunFilter;

#[instrument(level = "debug", skip(plugins, direct_etiquettes, filter))]
pub(super) fn resolved_etiquettes(
    plugins: &[&'static dyn Plugin],
    direct_etiquettes: &[&'static dyn Etiquette],
    filter: &dyn RunFilter,
) -> Vec<&'static dyn Etiquette> {
    let plugin_slice: Vec<&'static dyn Plugin> = if plugins.is_empty() {
        Vec::new()
    } else {
        selected_plugins(plugins, filter.plugins())
    };

    let mut merged = if plugin_slice.is_empty() {
        direct_etiquettes.to_vec()
    } else {
        etiquettes_from_plugins(&plugin_slice)
    };

    for etiquette in direct_etiquettes {
        if !merged
            .iter()
            .any(|existing| existing.id() == etiquette.id())
        {
            merged.push(*etiquette);
        }
    }

    match filter.etiquettes() {
        Some(ids) => merged
            .into_iter()
            .filter(|etiquette| ids.iter().any(|id| id == etiquette.id()))
            .collect(),
        None => merged,
    }
}

#[instrument(level = "debug", skip(etiquettes))]
pub(super) fn dedupe_loaders(etiquettes: &[&'static dyn Etiquette]) -> Vec<&'static dyn Loader> {
    dedupe_hooks(
        etiquettes
            .iter()
            .flat_map(|etiquette| etiquette.loaders().iter().copied()),
    )
}

#[instrument(level = "debug", skip(etiquettes, loaders))]
pub(super) fn dedupe_enrichers(
    etiquettes: &[&'static dyn Etiquette],
    loaders: &[&'static dyn Loader],
) -> Vec<&'static dyn IrEnricher> {
    static PATH_INDEX: PathIndexEnricher = PathIndexEnricher;
    static SYN_DOC_LINK: SynDocLinkEnricher = SynDocLinkEnricher;
    let mut out = dedupe_hooks(
        etiquettes
            .iter()
            .flat_map(|etiquette| etiquette.enrichers().iter().copied()),
    );
    let dual_inventory = loaders_include_source_and_rustdoc(loaders);
    #[cfg(feature = "rustdoc")]
    let has_rustdoc = loaders
        .iter()
        .any(|loader| loader.id() == RustdocLoader::ID);
    if !out.is_empty() || dual_inventory {
        #[cfg(feature = "rustdoc")]
        if has_rustdoc
            && !out
                .iter()
                .any(|enricher| enricher.id() == RustdocStructureEnricher::ID)
        {
            static RUSTDOC_STRUCTURE: RustdocStructureEnricher = RustdocStructureEnricher;
            out.insert(0, &RUSTDOC_STRUCTURE);
        }
        if !out
            .iter()
            .any(|enricher| enricher.id() == PathIndexEnricher::ID)
        {
            out.push(&PATH_INDEX);
        }
        if dual_inventory
            && !out
                .iter()
                .any(|enricher| enricher.id() == SynDocLinkEnricher::ID)
        {
            out.push(&SYN_DOC_LINK);
        }
    }
    out.sort_by_key(|enricher| enricher.priority());
    out
}

#[instrument(level = "debug", skip(loaders))]
fn loaders_include_source_and_rustdoc(loaders: &[&'static dyn Loader]) -> bool {
    #[cfg(feature = "rustdoc")]
    {
        let mut has_source = false;
        let mut has_rustdoc = false;
        for loader in loaders {
            if loader.id() == SourceLoader::ID {
                has_source = true;
            }
            if loader.id() == RustdocLoader::ID {
                has_rustdoc = true;
            }
        }
        has_source && has_rustdoc
    }
    #[cfg(not(feature = "rustdoc"))]
    {
        // Without `rustdoc`, there's no dual-inventory case to detect --
        // `loaders` is real input but genuinely unneeded in this branch.
        let _ = loaders;
        false
    }
}

#[instrument(level = "debug", skip(enricher, load_views))]
pub(super) fn select_load_view<'a>(
    enricher: &dyn IrEnricher,
    load_views: &'a HashMap<String, Box<dyn LoadView>>,
    crate_name: &str,
) -> CordialResult<&'a dyn LoadView> {
    let preferred = enricher.required_loader();
    if let Some(view) = load_views.get(&format!("{crate_name}:{preferred}")) {
        return Ok(view.as_ref());
    }
    if let Some(view) = load_views.get(&format!("{crate_name}:{}", SourceLoader::ID)) {
        return Ok(view.as_ref());
    }
    #[cfg(feature = "rustdoc")]
    if let Some(view) = load_views.get(&format!("{crate_name}:{}", RustdocLoader::ID)) {
        return Ok(view.as_ref());
    }
    Err(CordialError::invariant(format!(
        "no load view available for enricher `{}` on crate `{crate_name}`",
        enricher.id()
    )))
}

#[instrument(level = "debug", skip(etiquettes))]
pub(super) fn dedupe_probes(etiquettes: &[&'static dyn Etiquette]) -> Vec<&'static dyn Probe> {
    dedupe_hooks(
        etiquettes
            .iter()
            .flat_map(|etiquette| etiquette.probes().iter().copied()),
    )
}

#[instrument(level = "debug", skip(etiquettes))]
pub(super) fn dedupe_assessors(
    etiquettes: &[&'static dyn Etiquette],
) -> Vec<&'static dyn Assessor> {
    dedupe_hooks(
        etiquettes
            .iter()
            .flat_map(|etiquette| etiquette.assessors().iter().copied()),
    )
}

#[instrument(level = "debug", skip(etiquettes))]
pub(super) fn dedupe_workspace_assessors(
    etiquettes: &[&'static dyn Etiquette],
) -> Vec<&'static dyn WorkspaceAssessor> {
    dedupe_hooks(
        etiquettes
            .iter()
            .flat_map(|etiquette| etiquette.workspace_assessors().iter().copied()),
    )
}

#[instrument(level = "debug", skip(etiquettes))]
pub(super) fn dedupe_reporters(
    etiquettes: &[&'static dyn Etiquette],
) -> Vec<&'static dyn Reporter> {
    dedupe_hooks(
        etiquettes
            .iter()
            .flat_map(|etiquette| etiquette.reporters().iter().copied()),
    )
}

#[instrument(level = "debug", skip(items))]
fn dedupe_hooks<'a, T: Hook + ?Sized>(items: impl Iterator<Item = &'a T>) -> Vec<&'a T> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for item in items {
        if seen.insert(item.hook_id()) {
            out.push(item);
        }
    }
    out
}

trait Hook {
    fn hook_id(&self) -> &str;
}

impl Hook for dyn Loader {
    fn hook_id(&self) -> &str {
        self.id()
    }
}

impl Hook for dyn IrEnricher {
    fn hook_id(&self) -> &str {
        self.id()
    }
}

impl Hook for dyn Probe {
    fn hook_id(&self) -> &str {
        self.id()
    }
}

impl Hook for dyn Assessor {
    fn hook_id(&self) -> &str {
        self.id()
    }
}

impl Hook for dyn WorkspaceAssessor {
    fn hook_id(&self) -> &str {
        self.id()
    }
}

impl Hook for dyn Reporter {
    #[instrument(level = "trace", skip(self))]
    fn hook_id(&self) -> &str {
        self.id()
    }
}

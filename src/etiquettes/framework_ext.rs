//! Amenable ext (third-party crate) registry coverage — the
//! `amenable_ext` counterpart of `etiquettes::framework_std`'s
//! `amenable-std` bundle.
//!
//! **What.** Workspace-scoped coverage etiquette for a target crate's
//! registered carriers: how much of its inventory has `ExtStandard<T>`
//! evidence and verifier witnesses in `amenable_ext`.
//!
//! **Why.** Same denominator question `amenable-std` asks, one target
//! crate at a time via a shadow-dep rustdoc build instead of the shared
//! sysroot cache.
//!
//! **Flags.** `amenable-ext-{target}` inventories one target crate.
//! [`build_ext_etiquette`] is the one generic implementation every target
//! instantiates (jiff today) — see
//! `docs/planning/amenable-ext-targets-config.md`.
//!
//! **Ignores.** Source-quality findings, local wrapper gaps, and project
//! trait coverage belong to other etiquettes.
//!
//! **Outputs.** `{store}/findings/amenable-ext-{target}.checklist.md` and
//! that target's coverage inventory artifacts.
//!
//! **Config.** Run `cordial coverage`. Requires the `amenable_ext`
//! feature (which requires `amenable_std` + `shadow` — see
//! `docs/AMENABLE_EXT_COVERAGE.md` in the `amenable` repo for the full
//! design). Builds the target's rustdoc JSON via a shadow-dep build the
//! first time it's needed, then reuses the cache.

mod assessor;
mod probe;
mod reporter;
mod row;

use std::sync::LazyLock;

use tracing::instrument;

pub use assessor::ExtAssessor;
pub use probe::ExtScopeProbe;
pub use reporter::ExtReporter;
pub use row::{ext_etiquette_id, ext_report_from_findings};

use crate::etiquette::{EtiquetteExplain, EtiquetteHooks, EtiquetteRuleExplain, StaticEtiquette};
use crate::hooks::{Assessor, Probe, Reporter};
use crate::objects::Rule;

/// Build the amenable-ext etiquette for `target` (e.g. `"jiff"`).
///
/// `StaticEtiquette`/`EtiquetteExplain`/`EtiquetteRuleExplain` all hold
/// owned `String`/`Vec` fields now, built through one plain constructor
/// shared with every built-in — no leaking needed for any of that.
/// `EtiquetteHooks` still needs `&'static dyn Trait` slices: an accepted,
/// explicitly-exempted pattern for crate-local trait-object tables
/// (`ANTIPATTERN-STRUCT-STATIC-REF-001`'s own carve-out), unrelated to the
/// owned-text fields above. The probe/assessor/reporter instances are
/// leaked once per call to get that `'static` lifetime — small, bounded
/// by the number of configured targets, and reclaimed at process exit
/// (this is a CLI, not a long-running server).
#[instrument(level = "debug")]
pub fn build_ext_etiquette(target: &str) -> StaticEtiquette {
    let id = ext_etiquette_id(target);

    let probe: &'static dyn Probe = Box::leak(Box::new(ExtScopeProbe::new(target)));
    let assessor: &'static dyn Assessor = Box::leak(Box::new(ExtAssessor::new(target)));
    let reporter: &'static dyn Reporter = Box::leak(Box::new(ExtReporter::new(target)));

    let probes: &'static [&'static dyn Probe] = Box::leak(Box::new([probe]));
    let assessors: &'static [&'static dyn Assessor] = Box::leak(Box::new([assessor]));
    let reporters: &'static [&'static dyn Reporter] = Box::leak(Box::new([reporter]));

    // Derived from the real `ExtRowRule`, not re-derived independently —
    // that duplication is exactly how the explain page's summary once
    // drifted from the rule's own description (lost the capitalization
    // `ExtRowRule::new` applies to `target`).
    let row_rule = row::ExtRowRule::new(target);
    let rule = EtiquetteRuleExplain::new(
        Rule::id(&row_rule).to_string(),
        Rule::description(&row_rule).to_string(),
    );

    let explain = EtiquetteExplain::new(
        format!(
            "How much of {target}'s registered carrier surface is in the amenable_ext registry?"
        ),
        "Same registry-coverage question amenable-std asks, for a third-party target crate via a shadow-dep rustdoc build instead of the shared sysroot cache.",
        "Workspace-scoped; no source loaders. Builds/reuses the target's shadow-dep rustdoc cache. Feature amenable_ext.",
        format!("`[{id}] enabled = false` in cordial.toml."),
        vec![rule],
    );

    StaticEtiquette::new(
        id,
        format!("Amenable ext ({target}) coverage"),
        EtiquetteHooks::new(&[], &[], probes, assessors, None, reporters),
        true,
        explain,
    )
}

/// Workspace-scoped amenable ext (jiff) registry coverage etiquette — the
/// one target instantiated today. Lazily built once, since its fields are
/// no longer `const`-eligible.
pub static AMENABLE_EXT_JIFF_ETIQUETTE: LazyLock<StaticEtiquette> =
    LazyLock::new(|| build_ext_etiquette("jiff"));

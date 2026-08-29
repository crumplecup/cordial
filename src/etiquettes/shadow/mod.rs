//! Shadow crates that should mirror upstream items.
//!
//! **What.** Pairs an upstream crate with its shadow (interface) crate and
//! reports types/methods that exist upstream but are not mirrored, including
//! a workspace-level pass across crate boundaries.
//!
//! **Why.** Shadow crates are the elicitation adapter for crates we do not
//! own. Missing mirrors mean the tracked target is incomplete even when
//! rustdoc for the upstream crate is present.
//!
//! **How to use.** `cordial build rustdoc`, then `cordial coverage` (feature
//! `shadow` / `elicitation`). Artifacts: `{store}/findings/shadow-*.checklist.md`
//! and pair/gap CSVs. Register [`SHADOW_ETIQUETTE`].

mod assessor;
mod probe;
mod reporter;
mod types;
mod workspace_assessor;

pub use assessor::ShadowAssessor;
pub use probe::MissingShadowMirrorProbe;
pub use reporter::{
    ShadowCsvReporter, ShadowGapsCsvReporter, ShadowMethodChecklistReporter, ShadowPairCsvReporter,
};
pub use workspace_assessor::CrossCrateShadowWorkspaceAssessor;

use crate::etiquette::{EtiquetteExplain, EtiquetteRuleExplain, StaticEtiquette};
use crate::{RustdocLoader, ShadowLinkEnricher};

static RUSTDOC_LOADER: RustdocLoader = RustdocLoader;
static SHADOW_LINK: ShadowLinkEnricher = ShadowLinkEnricher;
static MISSING_MIRROR_PROBE: MissingShadowMirrorProbe = MissingShadowMirrorProbe;
static SHADOW_ASSESSOR: ShadowAssessor = ShadowAssessor;
static CROSS_CRATE_SHADOW_WORKSPACE_ASSESSOR: CrossCrateShadowWorkspaceAssessor =
    CrossCrateShadowWorkspaceAssessor;
static SHADOW_CSV: ShadowCsvReporter = ShadowCsvReporter;
static SHADOW_PAIR_CSV: ShadowPairCsvReporter = ShadowPairCsvReporter;
static SHADOW_GAPS_CSV: ShadowGapsCsvReporter = ShadowGapsCsvReporter;
static SHADOW_METHOD_CHECKLIST: ShadowMethodChecklistReporter = ShadowMethodChecklistReporter;

static LOADERS: &[&'static dyn crate::Loader] = &[&RUSTDOC_LOADER];
static ENRICHERS: &[&'static dyn crate::IrEnricher] = &[&SHADOW_LINK];
static PROBES: &[&'static dyn crate::Probe] = &[&MISSING_MIRROR_PROBE];
static ASSESSORS: &[&'static dyn crate::Assessor] = &[&SHADOW_ASSESSOR];
static WORKSPACE_ASSESSORS: &[&'static dyn crate::WorkspaceAssessor] =
    &[&CROSS_CRATE_SHADOW_WORKSPACE_ASSESSOR];
static REPORTERS: &[&'static dyn crate::Reporter] = &[
    &SHADOW_CSV,
    &SHADOW_PAIR_CSV,
    &SHADOW_GAPS_CSV,
    &SHADOW_METHOD_CHECKLIST,
];

/// Built-in shadow mirror coverage etiquette bundle.
pub static SHADOW_ETIQUETTE: StaticEtiquette = StaticEtiquette {
    id: "shadow",
    name: "Shadow mirrors",
    loaders: LOADERS,
    enrichers: ENRICHERS,
    probes: PROBES,
    assessors: ASSESSORS,
    workspace_assessors: Some(WORKSPACE_ASSESSORS),
    reporters: REPORTERS,
    is_coverage: true,
    explain: EtiquetteExplain {
        summary: "Do shadow crates mirror upstream items?",
        why: "Shadow crates are the elicitation adapter for crates we do not own. Missing mirrors mean the tracked target is incomplete even when rustdoc for the upstream crate is present.",
        logic: "Pairs an upstream crate with its shadow crate and reports types/methods that exist upstream but are not mirrored, including a workspace-level pass. Needs cordial build rustdoc.",
        opt_out: "`[shadow] enabled = false` in cordial.toml.",
        rules: &[EtiquetteRuleExplain {
            id: "SHADOW-MISSING-MIRROR",
            summary: "Upstream item lacks a shadow mirror",
        }],
    },
};

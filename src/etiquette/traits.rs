//! Core etiquette trait.

use crate::hooks::{Assessor, IrEnricher, Loader, Probe, Reporter, WorkspaceAssessor};

use super::explain::EtiquetteExplain;

/// Named bundle of cordial hook implementations.
///
/// An etiquette is the smallest named standard a user can run or explain. It
/// does not own scheduling; the session deduplicates hooks by id, invokes them
/// in pipeline order, and writes any reporter artifacts into the project store.
pub trait Etiquette: Send + Sync {
    /// Stable identifier for command filters, reports, and deduplication.
    fn id(&self) -> &str;
    /// Human-readable display name for reports and `cordial explain`.
    fn name(&self) -> &str;

    /// Why this check exists, what it flags, and how to opt out.
    ///
    /// Required: a new etiquette that forgets this is a compile error, not a
    /// silent gap. See `docs/planning/etiquette-explain.md`.
    fn explain(&self) -> EtiquetteExplain;

    /// Loaders that populate IR for this etiquette.
    ///
    /// Return an empty slice only when the etiquette reads facts added by other
    /// registered hooks or uses workspace-only assessment.
    fn loaders(&self) -> &[&dyn Loader];
    /// Enrichers that run after loaders.
    ///
    /// Enrichers should add facts to the IR, not findings.
    fn enrichers(&self) -> &[&dyn IrEnricher];
    /// Probes that attach markers to the IR.
    ///
    /// Probes should emit observations. Judgment belongs in assessors.
    fn probes(&self) -> &[&dyn Probe];
    /// Assessors that turn markers into findings.
    ///
    /// Return an empty slice for inventory/report-only etiquettes.
    fn assessors(&self) -> &[&dyn Assessor];
    /// Optional workspace-scoped assessors; empty by default.
    ///
    /// Use these for cross-crate rules that cannot be judged against one crate
    /// graph at a time.
    fn workspace_assessors(&self) -> &[&dyn WorkspaceAssessor] {
        &[]
    }
    /// Reporters that render findings into artifacts.
    ///
    /// Artifact names should be stable because users view, diff, and script
    /// against files in the store.
    fn reporters(&self) -> &[&dyn Reporter];

    /// True for trait-impl / framework coverage hook bundles.
    ///
    /// Coverage etiquettes are routed with coverage plugins and do not
    /// contribute to the source-quality rollup.
    fn is_coverage(&self) -> bool {
        false
    }
}

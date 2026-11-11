use super::{InternalErrorComplianceFinding, InternalErrorComplianceId};

use tracing::instrument;
/// Compliance scan output for one crate.
#[derive(Debug, Clone, PartialEq, Eq, Default, derive_new::new, derive_getters::Getters)]
pub struct InternalErrorComplianceReport {
    crate_name: String,
    findings: Vec<InternalErrorComplianceFinding>,
}

impl InternalErrorComplianceReport {
    /// Findings that stringify a foreign error.
    #[instrument(level = "trace", skip(self))]
    pub fn stringify_count(&self) -> usize {
        self.findings()
            .iter()
            .filter(|finding| finding.rule_id() == InternalErrorComplianceId::StringifyForeign001)
            .count()
    }

    /// Findings that discard a typed error.
    #[instrument(level = "trace", skip(self))]
    pub fn discard_count(&self) -> usize {
        self.findings()
            .iter()
            .filter(|finding| finding.rule_id() == InternalErrorComplianceId::DiscardTyped001)
            .count()
    }

    /// Findings about the shape of a `source` accessor.
    #[instrument(level = "trace", skip(self))]
    pub fn source_shape_count(&self) -> usize {
        self.findings()
            .iter()
            .filter(|finding| finding.rule_id() == InternalErrorComplianceId::SourceShape001)
            .count()
    }

    /// Fields with storage that conflicts with the selected error policy.
    #[instrument(level = "trace", skip(self))]
    pub fn field_strategy_count(&self) -> usize {
        self.findings()
            .iter()
            .filter(|finding| finding.rule_id() == InternalErrorComplianceId::FieldStrategy001)
            .count()
    }

    /// Findings about `#[track_caller]` on source accessors.
    #[instrument(level = "trace", skip(self))]
    pub fn source_track_caller_count(&self) -> usize {
        self.findings()
            .iter()
            .filter(|finding| {
                matches!(
                    finding.rule_id(),
                    InternalErrorComplianceId::SourceTrackCaller001
                        | InternalErrorComplianceId::MapErrTrackCaller001
                )
            })
            .count()
    }

    /// Findings about the error-type architecture.
    #[instrument(level = "trace", skip(self))]
    pub fn architecture_count(&self) -> usize {
        self.findings()
            .iter()
            .filter(|finding| {
                matches!(
                    finding.rule_id(),
                    InternalErrorComplianceId::ArchParent001
                        | InternalErrorComplianceId::ArchKindBox001
                        | InternalErrorComplianceId::ArchKindVariant001
                        | InternalErrorComplianceId::ArchOrphanSource001
                )
            })
            .count()
    }
}

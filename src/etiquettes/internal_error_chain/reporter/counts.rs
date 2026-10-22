//! Count compliance classes for the error chain summary.

use tracing::instrument;

use super::{InternalErrorChainRow, compliance_rows};
use crate::etiquettes::internal_error_chain::types::InternalErrorComplianceId;

#[instrument(level = "debug", skip(rows))]
pub(super) fn compliance_counts(
    rows: &[InternalErrorChainRow],
) -> (usize, usize, usize, usize, usize, usize) {
    let mut stringify = 0usize;
    let mut discard = 0usize;
    let mut source_shape = 0usize;
    let mut field_strategy = 0usize;
    let mut track_caller = 0usize;
    let mut architecture = 0usize;
    for row in compliance_rows(rows) {
        match row.rule_id.as_str() {
            id if id == InternalErrorComplianceId::StringifyForeign001.as_str() => stringify += 1,
            id if id == InternalErrorComplianceId::DiscardTyped001.as_str() => discard += 1,
            id if id == InternalErrorComplianceId::SourceShape001.as_str() => source_shape += 1,
            id if id == InternalErrorComplianceId::FieldStrategy001.as_str() => field_strategy += 1,
            id if id == InternalErrorComplianceId::SourceTrackCaller001.as_str()
                || id == InternalErrorComplianceId::MapErrTrackCaller001.as_str() =>
            {
                track_caller += 1;
            }
            id if id == InternalErrorComplianceId::ArchParent001.as_str()
                || id == InternalErrorComplianceId::ArchKindBox001.as_str()
                || id == InternalErrorComplianceId::ArchKindVariant001.as_str()
                || id == InternalErrorComplianceId::ArchKindFanout001.as_str()
                || id == InternalErrorComplianceId::ArchOrphanSource001.as_str() =>
            {
                architecture += 1;
            }
            _ => {}
        }
    }
    (
        stringify,
        discard,
        source_shape,
        field_strategy,
        track_caller,
        architecture,
    )
}

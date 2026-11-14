//! Conversion units collected from a crate's type index.

use tracing::instrument;

use crate::FileSnapshot;
use crate::error::CordialResult;

use super::ErrorConversionUnit;
use super::collection::collect_conversion_units;
use super::resolution::CrateTypeIndex;

/// The conversion unit of every candidate field in the crate, in declaration
/// order. A function of the crate's files and its type index, so it can be a
/// query over the index.
#[instrument(level = "debug", skip(files, index), err(level = "warn"))]
#[hotpath::measure]
pub(crate) fn conversion_units(
    files: &[FileSnapshot],
    index: &CrateTypeIndex,
) -> CordialResult<Vec<ErrorConversionUnit>> {
    Ok(collect_conversion_units(files, index)?
        .into_iter()
        .map(|(unit, _)| unit)
        .collect())
}

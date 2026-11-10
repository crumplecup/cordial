//! Constructor initializer recognition for conservative error field edits.

use tracing::instrument;

use crate::config::ErrorFieldStrategy;

use super::Edit;

mod native;
mod native_bindings;
mod native_tuples;
mod native_values;
mod projection;
mod replacements;

pub(super) use projection::InitializerBatchInput;
pub(crate) use projection::InitializerFileFacts;
use replacements::InitializerExpressionFact;

/// Constructor sites and provenance recorded in the shared IR before apply.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) struct InitializerFacts {
    sites: Vec<Option<InitializerExpressionFact>>,
    found: usize,
    unresolved: bool,
}

impl InitializerFacts {
    #[instrument(level = "trace", skip(self, source, strategy))]
    pub(super) fn edits(&self, source: &str, strategy: ErrorFieldStrategy) -> Option<Vec<Edit>> {
        (self.found > 0 && !self.unresolved)
            .then(|| {
                self.sites
                    .iter()
                    .map(|site| site.as_ref()?.edit(source, strategy))
                    .collect()
            })
            .flatten()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) enum FieldKind {
    CallSiteFile,
    StaticString,
}

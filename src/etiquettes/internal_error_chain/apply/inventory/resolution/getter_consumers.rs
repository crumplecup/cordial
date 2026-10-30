//! Query shared crate receiver evidence for candidate-specific field closure.

use std::path::Path;
use tracing::instrument;

use super::super::super::getter_uses;
use super::CrateTypeIndex;

impl CrateTypeIndex {
    #[instrument(level = "trace", skip(self, path, usage, span))]
    pub(in crate::etiquettes::internal_error_chain::apply) fn unrelated_getter_field(
        &self,
        path: &Path,
        owner: &str,
        usage: &Path,
        span: proc_macro2::Span,
    ) -> bool {
        getter_uses::unrelated_field(&self.getter_uses, path, owner, usage, span)
    }

    #[instrument(level = "trace", skip(self, path, usage, span))]
    pub(in crate::etiquettes::internal_error_chain::apply) fn unrelated_getter_macro(
        &self,
        path: &Path,
        owner: &str,
        usage: &Path,
        span: proc_macro2::Span,
    ) -> bool {
        getter_uses::unrelated_macro(&self.getter_uses, path, owner, usage, span)
    }
}

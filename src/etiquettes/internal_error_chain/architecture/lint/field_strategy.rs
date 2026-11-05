//! Field storage policy for types that implement `Error`.

use tracing::instrument;

use crate::config::ErrorFieldStrategy;
use crate::error::CordialResult;
use crate::etiquettes::internal_error_chain::types::{
    InternalErrorComplianceFinding, InternalErrorComplianceId,
};

use super::super::catalog::Catalog;
use super::{FindingSite, SiteClassifier};

impl Catalog {
    #[instrument(level = "debug", skip(self, strategy, findings))]
    pub(super) fn emit_field_strategy_findings(
        &self,
        strategy: ErrorFieldStrategy,
        findings: &mut Vec<InternalErrorComplianceFinding>,
    ) -> CordialResult<()> {
        if strategy == ErrorFieldStrategy::Owned {
            return Ok(());
        }
        for field in self.fields() {
            let is_error_architecture = self.impls_error(field.owner())
                || self
                    .enums()
                    .get(field.owner())
                    .is_some_and(|item| self.is_error_kind(item));
            if !is_error_architecture || field.name() == "kind" {
                continue;
            }
            let issue = if field.name() == "file" && !field.ty().static_str() {
                Some("store the call-site file as `&'static str` from `Location::caller().file()`")
            } else if field.ty().borrowed_string() {
                Some("borrow string contents as `&str`, not `&String`")
            } else if !field.ty().is_copy(self.copy_impls()) {
                Some(
                    "borrow this non-Copy field; thread a lifetime through the error type as needed",
                )
            } else {
                None
            };
            let Some(issue) = issue else { continue };
            findings.push(self.finding(
                InternalErrorComplianceId::FieldStrategy001,
                FindingSite::new(
                    field.context().clone(),
                    field.file().clone(),
                    field.line(),
                    format!("{}: {} — {issue}", field.name(), field.ty().label()),
                ),
                SiteClassifier::default(),
            )?);
        }
        Ok(())
    }
}

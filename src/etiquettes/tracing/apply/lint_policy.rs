//! Restrict recipe repairs to the rules enabled for the edited file.

use super::super::types::TracingRuleKind;
use crate::{CordialConfig, LintLocation};

#[derive(derive_new::new)]
pub(super) struct ApplyLintPolicy<'a> {
    config: &'a CordialConfig,
    location: LintLocation<'a>,
}

impl ApplyLintPolicy<'_> {
    #[tracing::instrument(level = "trace", skip(self, kind))]
    pub(super) fn enabled(&self, kind: TracingRuleKind) -> bool {
        self.config
            .lint_enabled("tracing", kind.rule_id(), self.location)
    }
}

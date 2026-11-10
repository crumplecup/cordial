//! Parser-independent antipattern evidence, evaluated against current policy.

use super::contract_bounds::ContractClause;
use super::scan::truncate_snippet;
use super::scan_crate::AntipatternWorkspaceFacts;
use super::types::AntipatternSiteRecord;
use crate::config::{ErrorFieldStrategy, StaticRefStrategy};
use crate::{CordialResult, FileSnapshot};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters, derive_new::new,
)]
pub(crate) struct AntipatternCatalog {
    snapshots: Vec<FileSnapshot>,
    syntax: AntipatternSyntaxFacts,
    contracts: Vec<ContractClause>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_new::new)]
pub(crate) struct AntipatternSyntaxFacts {
    sites: Vec<AntipatternCandidate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum AntipatternCandidate {
    Record(AntipatternSiteRecord),
    StaticField(StaticField),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_new::new)]
pub(crate) struct StaticField {
    site: AntipatternSiteRecord,
    error_type: bool,
    shape: StaticFieldShape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum StaticFieldShape {
    Location,
    String,
    Other,
}

impl AntipatternCatalog {
    #[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
    pub(crate) fn assess(
        &self,
        crate_name: &str,
        facts: &AntipatternWorkspaceFacts,
        static_strategy: StaticRefStrategy,
        error_strategy: ErrorFieldStrategy,
    ) -> CordialResult<Vec<AntipatternSiteRecord>> {
        let mut records = self
            .syntax
            .assess(static_strategy, error_strategy, &HashMap::new())?;
        records.extend(facts.assess_contracts(&self.contracts));
        records.extend(facts.version_records(crate_name).iter().cloned());
        records.sort_by(|a, b| {
            a.file()
                .cmp(b.file())
                .then(a.line().cmp(&b.line()))
                .then(a.context().cmp(b.context()))
                .then(a.snippet().cmp(b.snippet()))
        });
        Ok(records)
    }
}

impl AntipatternSyntaxFacts {
    #[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
    pub(super) fn assess(
        &self,
        static_strategy: StaticRefStrategy,
        error_strategy: ErrorFieldStrategy,
        skip_paths: &HashMap<String, Vec<String>>,
    ) -> CordialResult<Vec<AntipatternSiteRecord>> {
        let mut records = Vec::new();
        for candidate in &self.sites {
            let field = match candidate {
                AntipatternCandidate::StaticField(field) => field,
                AntipatternCandidate::Record(site) => {
                    records.push(site.clone());
                    continue;
                }
            };
            if error_strategy == ErrorFieldStrategy::Borrowed
                && field.error_type
                && field.shape != StaticFieldShape::Location
            {
                continue;
            }
            let site = &field.site;
            records.push(
                AntipatternSiteRecord::builder()
                    .rule_id(site.rule_id())
                    .context(site.context().clone())
                    .file(site.file().clone())
                    .line(site.line())
                    .snippet(field.guidance(static_strategy))
                    .build()?,
            );
        }
        records.retain(|site| {
            !skip_paths
                .get(site.rule_id().as_str())
                .is_some_and(|paths| paths.iter().any(|prefix| site.file().starts_with(prefix)))
        });
        Ok(records)
    }
}

impl StaticField {
    #[tracing::instrument(level = "debug", skip(self, strategy))]
    fn guidance(&self, strategy: StaticRefStrategy) -> String {
        let found = self.site.snippet();
        if self.shape == StaticFieldShape::Location {
            return "copy `file` and `line` from Location; do not store &'static Location".into();
        }
        let advice = match (self.shape, strategy) {
            (StaticFieldShape::String, StaticRefStrategy::String) => {
                "replace &'static str fields with owned String data"
            }
            (StaticFieldShape::String, StaticRefStrategy::Cow) => {
                "replace &'static str fields with Cow<'static, str>"
            }
            (StaticFieldShape::String, StaticRefStrategy::Const) => {
                "const/static only for const-only types; otherwise use Cow<'static, str>"
            }
            (_, StaticRefStrategy::String) => "replace static reference fields with owned data",
            (_, StaticRefStrategy::Cow) => {
                "use Cow or a domain wrapper only when borrowing is intentional"
            }
            (_, StaticRefStrategy::Const) => {
                "const/static only for const-only types; otherwise own the data"
            }
        };
        truncate_snippet(&format!("{found}; {advice}"), 128)
    }
}

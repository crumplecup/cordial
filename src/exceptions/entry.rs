//! Exception entries and finding suppression wrappers.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::error::{CordialError, CordialResult};
use crate::objects::{Disposition, Finding, MapFindingSink, Rule};

use super::paths::{normalize_rel_path, paths_match};

/// One documented exception row in `{store}/exceptions/{etiquette}/{crate}.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct ExceptionEntry {
    /// Path relative to the crate root.
    file: String,
    /// When set, only findings on this line match.
    #[getter(copy)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    line: Option<u32>,
    /// When set, only findings with this rule id match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    rule_id: Option<String>,
    /// When set, only findings with this context/qualified name match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    context: Option<String>,
    /// Human-readable explanation shown in reports.
    reason: String,
}

/// Loaded exception patch set for one etiquette and crate.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExceptionSet {
    entries: Vec<ExceptionEntry>,
}

impl ExceptionSet {
    /// Build a set from these entries.
    #[instrument(level = "debug", skip(entries), ret)]
    pub fn from_entries(entries: Vec<ExceptionEntry>) -> Self {
        Self { entries }
    }

    /// Whether this set has no entries.
    #[instrument(level = "trace", skip(self))]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Reason of the first entry that matches this finding, if any.
    #[instrument(level = "trace", skip(self, finding))]
    pub fn match_reason(&self, finding: &dyn Finding) -> Option<&str> {
        self.entries
            .iter()
            .find(|entry| entry.matches(finding))
            .map(|entry| entry.reason.as_str())
    }
}

impl ExceptionEntry {
    /// Construct a new value.
    #[instrument(level = "debug", skip(file, reason), ret)]
    pub fn new(file: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            file: file.into(),
            line: None,
            rule_id: None,
            context: None,
            reason: reason.into(),
        }
    }

    /// Return a copy with `line` set.
    #[instrument(level = "trace", skip(self))]
    pub fn with_line(self, line: u32) -> Self {
        Self {
            line: Some(line),
            ..self
        }
    }

    /// Return a copy with `rule_id` set.
    #[instrument(level = "trace", skip(self, rule_id))]
    pub fn with_rule_id(self, rule_id: impl Into<String>) -> Self {
        Self {
            rule_id: Some(rule_id.into()),
            ..self
        }
    }

    /// Return a copy with `context` set.
    #[instrument(level = "trace", skip(self, context))]
    pub fn with_context(self, context: impl Into<String>) -> Self {
        Self {
            context: Some(context.into()),
            ..self
        }
    }

    #[instrument(level = "debug", skip(self), err(level = "warn"))]
    pub(super) fn normalized_for_store(mut self) -> CordialResult<Self> {
        self.file = normalize_rel_path(std::path::Path::new(self.file.trim()));
        self.reason = self.reason.trim().to_string();
        if let Some(rule_id) = self.rule_id.as_mut() {
            *rule_id = rule_id.trim().to_string();
            if rule_id.is_empty() {
                self.rule_id = None;
            }
        }
        if let Some(context) = self.context.as_mut() {
            *context = context.trim().to_string();
            if context.is_empty() {
                self.context = None;
            }
        }
        require_nonempty("file", &self.file)?;
        require_nonempty("reason", &self.reason)?;
        Ok(self)
    }

    #[instrument(level = "debug", skip(self, finding))]
    fn matches(&self, finding: &dyn Finding) -> bool {
        let mut sink = MapFindingSink::default();
        finding.emit(&mut sink);
        let field = |name: &str| {
            sink.fields()
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.as_str())
        };

        let Some(file) = field("file") else {
            return false;
        };
        if !paths_match(&self.file, file) {
            return false;
        }
        if self
            .line
            .is_some_and(|line| field("line") != Some(&line.to_string()))
        {
            return false;
        }
        if self.rule_id.as_ref().is_some_and(|rule_id| {
            finding.rule().id() != rule_id && field("kind") != Some(rule_id.as_str())
        }) {
            return false;
        }
        if self
            .context
            .as_ref()
            .is_some_and(|context| field("context") != Some(context.as_str()))
        {
            return false;
        }
        true
    }
}

#[instrument(level = "debug")]
fn require_nonempty(label: &str, value: &str) -> CordialResult<()> {
    if value.trim().is_empty() {
        return Err(CordialError::invariant(format!(
            "{label} must not be empty"
        )));
    }
    Ok(())
}

/// Wrapper that overrides disposition and records a suppression reason.
pub struct FilteredFinding {
    inner: Box<dyn Finding>,
    disposition: Disposition,
    suppression_reason: Option<String>,
}

impl FilteredFinding {
    #[instrument(level = "debug", skip(inner, reason))]
    pub fn suppressed(inner: Box<dyn Finding>, reason: impl Into<String>) -> Self {
        Self {
            inner,
            disposition: Disposition::Suppressed,
            suppression_reason: Some(reason.into()),
        }
    }
}

impl Finding for FilteredFinding {
    #[instrument(level = "trace", skip(self))]
    fn rule(&self) -> &dyn Rule {
        self.inner.rule()
    }

    #[instrument(level = "trace", skip(self))]
    fn disposition(&self) -> Disposition {
        self.disposition
    }

    #[instrument(level = "trace", skip(self))]
    fn anchor(&self) -> &dyn crate::objects::IrAnchor {
        self.inner.anchor()
    }

    #[instrument(level = "trace", skip(self, sink))]
    fn emit(&self, sink: &mut dyn crate::objects::FindingSink) {
        self.inner.emit(sink);
        if let Some(reason) = &self.suppression_reason {
            sink.field("suppression_reason", reason);
        }
    }
}

/// Apply loaded exception sets, wrapping matching findings as suppressed.
#[instrument(level = "debug", skip(findings, sets))]
pub fn apply_exception_sets(
    findings: Vec<Box<dyn Finding>>,
    sets: &HashMap<String, ExceptionSet>,
) -> Vec<Box<dyn Finding>> {
    findings
        .into_iter()
        .map(|finding| {
            let category = finding.rule().category();
            if let Some(set) = sets.get(category)
                && let Some(reason) = set.match_reason(finding.as_ref())
            {
                return Box::new(FilteredFinding::suppressed(finding, reason.to_string()))
                    as Box<dyn Finding>;
            }
            finding
        })
        .collect()
}

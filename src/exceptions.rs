use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::error::CordialResult;
use crate::objects::{Disposition, Finding, MapFindingSink, Rule};
use crate::store::StoreLayout;

/// One documented exception row in `{store}/exceptions/{etiquette}/{crate}.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExceptionEntry {
    /// Path relative to the crate root.
    pub file: String,
    /// When set, only findings on this line match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    /// When set, only findings with this rule id match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule_id: Option<String>,
    /// When set, only findings with this context/qualified name match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    /// Human-readable explanation shown in reports.
    pub reason: String,
}

/// Loaded exception patch set for one etiquette and crate.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExceptionSet {
    entries: Vec<ExceptionEntry>,
}

impl ExceptionSet {
    #[instrument(level = "debug", ret)]
    pub fn from_entries(entries: Vec<ExceptionEntry>) -> Self {
        Self { entries }
    }

    #[instrument(level = "trace", skip(self))]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[instrument(level = "trace", skip(self, finding))]
    pub fn match_reason(&self, finding: &dyn Finding) -> Option<&str> {
        self.entries
            .iter()
            .find(|entry| entry.matches(finding))
            .map(|entry| entry.reason.as_str())
    }
}

impl ExceptionEntry {
    fn matches(&self, finding: &dyn Finding) -> bool {
        let mut sink = MapFindingSink::default();
        finding.emit(&mut sink);
        let field = |name: &str| {
            sink.fields
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
    fn rule(&self) -> &dyn Rule {
        self.inner.rule()
    }

    fn disposition(&self) -> Disposition {
        self.disposition
    }

    fn anchor(&self) -> &dyn crate::objects::IrAnchor {
        self.inner.anchor()
    }

    fn emit(&self, sink: &mut dyn crate::objects::FindingSink) {
        self.inner.emit(sink);
        if let Some(reason) = &self.suppression_reason {
            sink.field("suppression_reason", reason);
        }
    }
}

/// Load exceptions for one etiquette and crate.
///
/// Reads `{store}/exceptions/{etiquette}/{crate}.json` and, when present,
/// merges entries from the elicit_doc alias `{store}/quality/patches/{etiquette}/{crate}.json`.
#[instrument(level = "info", fields(crate_name = crate_name), err(level = "warn"))]
pub fn load_exceptions(
    store: &StoreLayout,
    etiquette_id: &str,
    crate_name: &str,
) -> CordialResult<ExceptionSet> {
    let file_name = format!("{crate_name}.json");
    let canonical = store.exceptions_dir().join(etiquette_id).join(&file_name);
    let alias = store
        .quality_patches_dir()
        .join(etiquette_id)
        .join(&file_name);

    let mut entries = Vec::new();
    if canonical.is_file() {
        entries.extend(parse_exception_file(&canonical)?);
    }
    if alias.is_file() {
        entries.extend(parse_exception_file(&alias)?);
    }
    Ok(ExceptionSet { entries })
}

fn parse_exception_file(path: &Path) -> CordialResult<Vec<ExceptionEntry>> {
    let bytes = std::fs::read(path)?;
    serde_json::from_slice::<Vec<ExceptionEntry>>(&bytes)
        .map_err(|err| crate::error::CordialError::json_parse(path.display().to_string(), err))
}

/// Load exception sets for all selected etiquettes.
#[instrument(level = "info", fields(crate_name = crate_name), err(level = "warn"))]
pub fn load_exception_sets(
    store: &StoreLayout,
    etiquette_ids: &[&str],
    crate_name: &str,
) -> CordialResult<HashMap<String, ExceptionSet>> {
    let mut sets = HashMap::new();
    for etiquette_id in etiquette_ids {
        sets.insert(
            (*etiquette_id).to_string(),
            load_exceptions(store, etiquette_id, crate_name)?,
        );
    }
    Ok(sets)
}

/// Apply loaded exception sets, wrapping matching findings as suppressed.
#[instrument(level = "debug", skip(findings))]
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

fn paths_match(patch_path: &str, finding_path: &str) -> bool {
    let patch = normalize_rel_path(Path::new(patch_path));
    let finding = normalize_rel_path(Path::new(finding_path));
    patch == finding || finding.ends_with(&patch) || finding.ends_with(&format!("/{patch}"))
}

fn normalize_rel_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

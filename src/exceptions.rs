use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::error::{CordialError, CordialResult};
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
    #[instrument(level = "debug", skip(entries), ret)]
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

    #[instrument(level = "debug", skip(self), ret)]
    pub fn with_line(self, line: u32) -> Self {
        Self {
            line: Some(line),
            ..self
        }
    }

    #[instrument(level = "debug", skip(self, rule_id), ret)]
    pub fn with_rule_id(self, rule_id: impl Into<String>) -> Self {
        Self {
            rule_id: Some(rule_id.into()),
            ..self
        }
    }

    #[instrument(level = "debug", skip(self, context), ret)]
    pub fn with_context(self, context: impl Into<String>) -> Self {
        Self {
            context: Some(context.into()),
            ..self
        }
    }

    #[instrument(level = "debug", skip(self, finding))]
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

/// Load exceptions for one etiquette and crate.
///
/// Reads `{store}/exceptions/{etiquette}/{crate}.json` and, when present,
/// merges entries from the elicit_doc alias `{store}/quality/patches/{etiquette}/{crate}.json`.
#[instrument(level = "info", skip(store), fields(crate_name = crate_name), err(level = "warn"))]
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

/// Canonical quality exception file: `{store}/exceptions/{etiquette}/{crate}.json`.
#[instrument(level = "trace", skip(store))]
pub fn exception_file_path(store: &StoreLayout, etiquette_id: &str, crate_name: &str) -> PathBuf {
    store
        .exceptions_dir()
        .join(etiquette_id)
        .join(format!("{crate_name}.json"))
}

/// Canonical coverage skip list: `{store}/patches/{patch_set}.json`.
#[instrument(level = "trace", skip(store))]
pub fn coverage_skip_file_path(store: &StoreLayout, patch_set: &str) -> PathBuf {
    store.patches_dir().join(format!("{patch_set}.json"))
}

/// One coverage skip-list row in `{store}/patches/{patch_set}.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoverageSkipEntry {
    /// Qualified path or type name to skip.
    pub path: String,
    /// Human-readable explanation shown in reports.
    pub reason: String,
}

impl CoverageSkipEntry {
    #[instrument(level = "debug", skip(path, reason), ret)]
    pub fn new(path: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            reason: reason.into(),
        }
    }
}

/// Result of appending one exception row to a store file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddExceptionOutcome {
    /// The row was written to `path`.
    Inserted { path: PathBuf },
    /// An identical row was already present at `path`.
    AlreadyPresent { path: PathBuf },
}

impl AddExceptionOutcome {
    #[instrument(level = "trace", skip(self))]
    pub fn path(&self) -> &Path {
        match self {
            Self::Inserted { path } | Self::AlreadyPresent { path } => path,
        }
    }

    #[instrument(level = "trace", skip(self))]
    pub fn inserted(&self) -> bool {
        matches!(self, Self::Inserted { .. })
    }
}

/// Append a quality exception to `{store}/exceptions/{etiquette}/{crate}.json`.
///
/// Creates the file when missing. An identical row is a no-op. Rows that
/// already live in the elicit_doc alias are left there and not duplicated.
#[instrument(level = "info", skip(store, entry), fields(crate_name = crate_name), err(level = "warn"))]
pub fn add_exception(
    store: &StoreLayout,
    etiquette_id: &str,
    crate_name: &str,
    entry: ExceptionEntry,
) -> CordialResult<AddExceptionOutcome> {
    require_nonempty("etiquette", etiquette_id)?;
    require_nonempty("crate_name", crate_name)?;
    let entry = normalize_quality_entry(entry)?;
    let canonical = exception_file_path(store, etiquette_id, crate_name);
    let alias = store
        .quality_patches_dir()
        .join(etiquette_id)
        .join(format!("{crate_name}.json"));

    let mut entries = if canonical.is_file() {
        parse_exception_file(&canonical)?
    } else {
        Vec::new()
    };
    if entries.contains(&entry) {
        return Ok(AddExceptionOutcome::AlreadyPresent { path: canonical });
    }
    if alias.is_file() {
        let alias_entries = parse_exception_file(&alias)?;
        if alias_entries.contains(&entry) {
            return Ok(AddExceptionOutcome::AlreadyPresent { path: alias });
        }
    }

    entries.push(entry);
    write_pretty_json(&canonical, &entries)?;
    Ok(AddExceptionOutcome::Inserted { path: canonical })
}

/// Append a coverage skip to `{store}/patches/{patch_set}.json`.
///
/// Existing objects keep unknown fields (for example `verifiers`). An
/// identical `path` in the canonical file or the `exceptions/` alias is a
/// no-op.
#[instrument(level = "info", skip(store, entry), err(level = "warn"))]
pub fn add_coverage_skip(
    store: &StoreLayout,
    patch_set: &str,
    entry: CoverageSkipEntry,
) -> CordialResult<AddExceptionOutcome> {
    require_nonempty("patch_set", patch_set)?;
    let entry = normalize_coverage_entry(entry)?;
    let canonical = coverage_skip_file_path(store, patch_set);
    let alias = store.exceptions_dir().join(format!("{patch_set}.json"));

    let mut rows = if canonical.is_file() {
        parse_json_array(&canonical)?
    } else {
        Vec::new()
    };
    if json_rows_contain_path(&rows, &entry.path) {
        return Ok(AddExceptionOutcome::AlreadyPresent { path: canonical });
    }
    if alias.is_file() && json_rows_contain_path(&parse_json_array(&alias)?, &entry.path) {
        return Ok(AddExceptionOutcome::AlreadyPresent { path: alias });
    }

    rows.push(serde_json::to_value(&entry)?);
    write_pretty_json(&canonical, &rows)?;
    Ok(AddExceptionOutcome::Inserted { path: canonical })
}

#[instrument(level = "debug", skip(path), err(level = "warn"))]
fn parse_exception_file(path: &Path) -> CordialResult<Vec<ExceptionEntry>> {
    let bytes = std::fs::read(path)?;
    serde_json::from_slice::<Vec<ExceptionEntry>>(&bytes)
        .map_err(|err| crate::error::CordialError::json_parse(path.display().to_string(), err))
}

/// Load exception sets for all selected etiquettes.
#[instrument(level = "info", skip(store), fields(crate_name = crate_name), err(level = "warn"))]
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

#[instrument(level = "debug")]
fn paths_match(patch_path: &str, finding_path: &str) -> bool {
    let patch = normalize_rel_path(Path::new(patch_path));
    let finding = normalize_rel_path(Path::new(finding_path));
    patch == finding || finding.ends_with(&patch) || finding.ends_with(&format!("/{patch}"))
}

/// Default repo-side registry directory name, relative to the project root.
pub const DEFAULT_EXCEPTIONS_REGISTRY: &str = ".cordial-exceptions";

/// Resolve a load/backup registry root against the project.
///
/// Absolute paths stay as given. Relative paths join the project root so
/// `cordial -p /repo exceptions load .elicit_doc-exceptions` works from any cwd.
#[instrument(level = "debug")]
pub fn resolve_exceptions_root(project_root: &Path, root: &Path) -> PathBuf {
    if root.is_absolute() {
        root.to_path_buf()
    } else {
        project_root.join(root)
    }
}

/// Backup curated exception files into `{backup_root}/{slug}/...`.
///
/// Writes `exceptions/`, `quality/patches/`, and `patches/` (coverage skip
/// lists). The last two keep elicit_doc's registry layout so a checkout like
/// elicitation's `.elicit_doc-exceptions/` loads without renaming.
#[instrument(level = "info", skip(store), err(level = "warn"))]
pub fn backup_exception_files(store: &StoreLayout, backup_root: &Path) -> CordialResult<usize> {
    store.ensure_dirs()?;
    let backup_slug_root = backup_root.join(&store.project_slug);
    let mut copied = 0usize;
    for (relative, from) in exception_subtrees(store) {
        copied += sync_exception_subtree(&from, &backup_slug_root.join(relative))?;
    }
    prune_empty_dirs_up_to(&backup_slug_root, backup_root)?;
    Ok(copied)
}

/// Load curated exception files from `{backup_root}/{slug}/...` into the store.
///
/// Replaces the matching store subtrees. Missing backup subtrees clear the
/// corresponding store dirs so the registry is the source of truth.
#[instrument(level = "info", skip(store), err(level = "warn"))]
pub fn load_exception_files(store: &StoreLayout, backup_root: &Path) -> CordialResult<usize> {
    let backup_slug_root = backup_root.join(&store.project_slug);
    if !backup_slug_root.is_dir() {
        return Err(CordialError::not_found(backup_slug_root));
    }
    store.ensure_dirs()?;
    let mut copied = 0usize;
    for (relative, to) in exception_subtrees(store) {
        copied += sync_exception_subtree(&backup_slug_root.join(relative), &to)?;
    }
    Ok(copied)
}

#[instrument(level = "trace", skip(store))]
fn exception_subtrees(store: &StoreLayout) -> [(&'static str, PathBuf); 3] {
    [
        ("exceptions", store.exceptions_dir()),
        ("quality/patches", store.quality_patches_dir()),
        ("patches", store.patches_dir()),
    ]
}

#[instrument(level = "debug", skip(from, to), err(level = "warn"))]
fn sync_exception_subtree(from: &Path, to: &Path) -> CordialResult<usize> {
    if to.exists() {
        fs::remove_dir_all(to)?;
    }
    if !from.exists() || (from.is_dir() && dir_is_empty(from)?) {
        return Ok(0);
    }
    fs::create_dir_all(to)?;
    copy_tree_overwrite(from, to)
}

#[instrument(level = "debug", skip(from, to), err(level = "warn"))]
fn copy_tree_overwrite(from: &Path, to: &Path) -> CordialResult<usize> {
    let mut copied = 0usize;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let src = entry.path();
        let dest = to.join(entry.file_name());
        if src.is_dir() {
            fs::create_dir_all(&dest)?;
            copied += copy_tree_overwrite(&src, &dest)?;
        } else if src.is_file() {
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&src, &dest)?;
            copied += 1;
        }
    }
    Ok(copied)
}

#[instrument(level = "debug", skip(start, stop), err(level = "warn"))]
fn prune_empty_dirs_up_to(start: &Path, stop: &Path) -> CordialResult<()> {
    let mut current = start.to_path_buf();
    while current.starts_with(stop) && current != stop {
        if !current.exists() || !dir_is_empty(&current)? {
            break;
        }
        fs::remove_dir(&current)?;
        let Some(parent) = current.parent() else {
            break;
        };
        current = parent.to_path_buf();
    }
    Ok(())
}

#[instrument(level = "trace", skip(path), err(level = "warn"))]
fn dir_is_empty(path: &Path) -> CordialResult<bool> {
    Ok(fs::read_dir(path)?.next().transpose()?.is_none())
}

#[instrument(level = "debug", skip(path))]
fn normalize_rel_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
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

#[instrument(level = "debug", skip(entry), err(level = "warn"))]
fn normalize_quality_entry(mut entry: ExceptionEntry) -> CordialResult<ExceptionEntry> {
    entry.file = normalize_rel_path(Path::new(entry.file.trim()));
    entry.reason = entry.reason.trim().to_string();
    if let Some(rule_id) = entry.rule_id.as_mut() {
        *rule_id = rule_id.trim().to_string();
        if rule_id.is_empty() {
            entry.rule_id = None;
        }
    }
    if let Some(context) = entry.context.as_mut() {
        *context = context.trim().to_string();
        if context.is_empty() {
            entry.context = None;
        }
    }
    require_nonempty("file", &entry.file)?;
    require_nonempty("reason", &entry.reason)?;
    Ok(entry)
}

#[instrument(level = "debug", skip(entry), err(level = "warn"))]
fn normalize_coverage_entry(mut entry: CoverageSkipEntry) -> CordialResult<CoverageSkipEntry> {
    entry.path = entry.path.trim().to_string();
    entry.reason = entry.reason.trim().to_string();
    require_nonempty("path", &entry.path)?;
    require_nonempty("reason", &entry.reason)?;
    Ok(entry)
}

#[instrument(level = "debug", skip(path), err(level = "warn"))]
fn parse_json_array(path: &Path) -> CordialResult<Vec<serde_json::Value>> {
    let bytes = fs::read(path)?;
    serde_json::from_slice::<Vec<serde_json::Value>>(&bytes)
        .map_err(|err| CordialError::json_parse(path.display().to_string(), err))
}

#[instrument(level = "trace", skip(rows))]
fn json_rows_contain_path(rows: &[serde_json::Value], path: &str) -> bool {
    rows.iter().any(|row| {
        row.get("path")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| value == path)
    })
}

#[instrument(level = "debug", skip(path, value), err(level = "warn"))]
fn write_pretty_json(path: &Path, value: &impl Serialize) -> CordialResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut body = serde_json::to_string_pretty(value)?;
    if !body.ends_with('\n') {
        body.push('\n');
    }
    fs::write(path, body)?;
    Ok(())
}

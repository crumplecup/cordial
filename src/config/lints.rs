//! Shared policy for built-in and plugin etiquettes.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Enable switches shared by every etiquette, including plugin-defined ids.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
#[serde(default, deny_unknown_fields)]
pub struct LintPolicy {
    /// Override the etiquette's existing global enable switch.
    enabled: Option<bool>,
    /// Override individual rule ids within this etiquette.
    rules: BTreeMap<String, bool>,
    /// Exempt individual rules on crate-relative files or directory prefixes.
    skip_paths: BTreeMap<String, Vec<String>>,
    /// Folder and file overrides, resolved from broadest to narrowest.
    paths: Vec<LintPathPolicy>,
}

/// A folder or file override. Paths match whole components, without globbing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
#[serde(deny_unknown_fields)]
pub struct LintPathPolicy {
    /// File or folder path relative to `base`.
    path: String,
    /// Path root; defaults to the crate root.
    #[serde(default)]
    base: LintPathBase,
    /// Optionally restrict this override to one Cargo crate name.
    #[serde(default, rename = "crate")]
    crate_name: Option<String>,
    /// Enable or disable the whole etiquette at this path.
    #[serde(default)]
    enabled: Option<bool>,
    /// Enable or disable individual rules at this path.
    #[serde(default)]
    rules: BTreeMap<String, bool>,
}

/// Root used to interpret a configured policy path.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LintPathBase {
    /// Relative to each crate's root directory.
    #[default]
    Crate,
    /// Relative to the workspace's root directory.
    Workspace,
}

/// Existing location evidence used to resolve a lint policy.
#[derive(Debug, Clone, Copy, derive_new::new, derive_getters::Getters)]
pub struct LintLocation<'a> {
    #[getter(copy)]
    workspace_root: &'a Path,
    #[getter(copy)]
    crate_root: Option<&'a Path>,
    #[getter(copy)]
    crate_name: &'a str,
    #[getter(copy)]
    file: Option<&'a Path>,
}

impl LintPolicy {
    /// Whether a rule or path override can enable an otherwise disabled etiquette.
    #[tracing::instrument(level = "trace", skip(self))]
    pub(crate) fn has_enables(&self) -> bool {
        self.rules.values().any(|enabled| *enabled)
            || self.paths.iter().any(|scope| {
                scope.enabled == Some(true) || scope.rules.values().any(|enabled| *enabled)
            })
    }

    #[tracing::instrument(level = "trace", skip(self, location))]
    pub(crate) fn resolve(&self, default: bool, rule: &str, location: LintLocation<'_>) -> bool {
        let mut enabled = self.rules.get(rule).copied().unwrap_or(default);
        let Some(file) = location.file.and_then(super::lint_paths::normalized) else {
            return enabled;
        };
        let absolute = if file.is_absolute() {
            file
        } else {
            match location.crate_root {
                Some(root) => root.join(file),
                None => return enabled,
            }
        };
        if self.skip_paths.get(rule).is_some_and(|paths| {
            paths.iter().any(|path| {
                location
                    .crate_root
                    .is_some_and(|root| super::lint_paths::matches(root, path, &absolute).is_some())
            })
        }) {
            enabled = false;
        }
        let mut matches: Vec<_> = self
            .paths
            .iter()
            .filter(|scope| {
                scope
                    .crate_name
                    .as_deref()
                    .is_none_or(|name| name == location.crate_name)
            })
            .filter_map(|scope| {
                let root = match scope.base {
                    LintPathBase::Crate => location.crate_root?,
                    LintPathBase::Workspace => location.workspace_root,
                };
                super::lint_paths::matches(root, &scope.path, &absolute).map(|depth| (depth, scope))
            })
            .collect();
        // Stable sort: equal paths retain declaration order, so the last wins.
        matches.sort_by_key(|(depth, _)| *depth);
        for (_, scope) in matches {
            enabled = scope
                .rules
                .get(rule)
                .copied()
                .or(scope.enabled)
                .unwrap_or(enabled);
        }
        enabled
    }
}

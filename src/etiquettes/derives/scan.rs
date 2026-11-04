//! Derive policy over loader-projected declaration and method evidence.
mod candidates;
mod walk;

use super::{
    path_inclusion::PathInclusionFacts,
    types::{DeriveRuleId, DeriveSiteRecord},
};
use crate::ir::{DeriveFileFacts, DeriveStructFacts};
use crate::{CordialResult, config::DerivesThresholds};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tracing::instrument;

#[instrument(
    level = "debug",
    skip(source, thresholds, path_inclusions, file),
    err(level = "warn")
)]
/// Scan one Rust source using loader-owned collection and ordinary graph policy.
pub fn scan_rust_source(
    source: &str,
    file: &Path,
    src_root: &Path,
    crate_root: &Path,
    thresholds: DerivesThresholds,
    path_inclusions: &PathInclusionFacts,
) -> CordialResult<Vec<DeriveSiteRecord>> {
    let facts = crate::ir::scan_derives_source(source, file, src_root, crate_root)?;
    select_records(&[facts], crate_root, thresholds, path_inclusions)
}

#[instrument(
    level = "debug",
    skip(files, thresholds, path_inclusions),
    err(level = "warn")
)]
pub(super) fn select_records(
    files: &[DeriveFileFacts],
    crate_root: &Path,
    thresholds: DerivesThresholds,
    path_inclusions: &PathInclusionFacts,
) -> CordialResult<Vec<DeriveSiteRecord>> {
    let mut findings = Vec::new();
    for file in files {
        let mut visitor = DeriveScanVisitor {
            file: crate_root.join(file.file()),
            crate_root: crate_root.to_path_buf(),
            module_prefix: file.module_prefix().clone(),
            structs: HashMap::new(),
            generated_builders: HashSet::new(),
            error_types: HashSet::new(),
            thresholds,
            path_inclusions,
            findings: Vec::new(),
            in_cfg_creusot_mod: false,
            error: None,
        };
        visitor.walk_items(file.items());
        if let Some(error) = visitor.error {
            return Err(error);
        }
        findings.extend(visitor.findings);
    }
    findings.sort_by(|a, b| {
        a.file()
            .cmp(b.file())
            .then(a.line().cmp(&b.line()))
            .then(a.qualified_name().cmp(b.qualified_name()))
    });
    Ok(findings)
}

/// Every fact needed to build one [`DeriveSiteRecord`], bundled so
/// [`DeriveScanVisitor::site`] takes one argument instead of seven.
#[derive(derive_new::new)]
struct SiteArgs {
    rule_id: DeriveRuleId,
    #[new(into)]
    struct_name: String,
    method_name: Option<String>,
    #[new(into)]
    qualified_local: String,
    #[new(into)]
    recommendation: String,
    line: u32,
    #[new(into)]
    evidence: String,
}

struct DeriveScanVisitor<'a> {
    file: PathBuf,
    crate_root: PathBuf,
    module_prefix: Vec<String>,
    structs: HashMap<String, DeriveStructFacts>,
    generated_builders: HashSet<String>,
    error_types: HashSet<String>,
    thresholds: DerivesThresholds,
    path_inclusions: &'a PathInclusionFacts,
    findings: Vec<DeriveSiteRecord>,
    /// Whether the item currently being walked sits inside a
    /// `#[cfg(creusot)]`-gated `mod`, even if that gate sits on some
    /// ancestor rather than the item itself -- the real, predominant
    /// shape in this workspace (`amenable_creusot`'s own `mirror`
    /// modules) gates once on the enclosing `mod`, never repeats
    /// `#[cfg(creusot)]` on every item inside it. `is_cfg_creusot`
    /// alone (checking only an item's own attrs) never matches that
    /// shape at all.
    in_cfg_creusot_mod: bool,
    error: Option<crate::error::CordialError>,
}

impl DeriveScanVisitor<'_> {
    /// True when some *other* crate splices this file in via `#[path]`
    /// without `needed_dep` -- the recommended derive wouldn't actually
    /// compile everywhere this file's content lands.
    #[instrument(level = "trace", skip(self))]
    fn blocked_by_path_inclusion(&self, needed_dep: &str) -> bool {
        if let Some(blocker) =
            self.path_inclusions
                .blocking_consumer(&self.file, &self.crate_root, needed_dep)
        {
            tracing::debug!(
                blocker,
                needed_dep,
                file = %self.file.display(),
                "derive recommendation blocked: consuming crate lacks the dependency"
            );
            return true;
        }
        false
    }

    #[instrument(level = "debug", skip(self))]
    fn qualify(&self, local: &str) -> String {
        if self.module_prefix.is_empty() {
            local.to_string()
        } else {
            format!("{}::{local}", self.module_prefix.join("::"))
        }
    }

    #[instrument(level = "trace", skip(self, args))]
    fn site(&mut self, args: SiteArgs) -> Option<DeriveSiteRecord> {
        if self.error.is_some() {
            return None;
        }
        let mut file = self.file.clone();
        if let Ok(rel) = file.strip_prefix(&self.crate_root) {
            file = rel.to_path_buf();
        }
        match DeriveSiteRecord::builder()
            .rule_id(args.rule_id)
            .struct_name(args.struct_name)
            .method_name(args.method_name)
            .qualified_name(self.qualify(&args.qualified_local))
            .recommendation(args.recommendation)
            .file(file)
            .line(args.line)
            .evidence(args.evidence)
            .build()
        {
            Ok(record) => Some(record),
            Err(error) => {
                self.error = Some(error);
                None
            }
        }
    }

    #[instrument(level = "debug", skip(self, args))]
    fn push_site(&mut self, args: SiteArgs) {
        if let Some(record) = self.site(args) {
            self.findings.push(record);
        }
    }
}

#[instrument(level = "debug")]
fn setter_field_name(method_name: &str) -> Option<&str> {
    method_name
        .strip_prefix("with_")
        .or_else(|| method_name.strip_prefix("set_"))
}

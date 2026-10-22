//! Parent / Kind / native-source findings from a completed catalog.

use std::collections::BTreeSet;
use std::path::PathBuf;

use super::super::types::{InternalErrorComplianceFinding, InternalErrorComplianceId};
use super::catalog::{Catalog, EnumInfo, StructInfo};
use crate::config::ErrorFieldStrategy;
use crate::error::CordialResult;

use tracing::instrument;

mod fanout;
mod field_strategy;
mod kind_variant;
mod track_caller;

/// Where a compliance finding sits and the message it carries -- grouped so
/// [`Catalog::finding`] stays a small call.
#[derive(Debug, Clone, derive_new::new)]
struct FindingSite {
    /// Qualified type path or extra locator for this site.
    context: String,
    /// Source file the offending item is declared in.
    file: PathBuf,
    /// Source line (1-based) of the offending item.
    line: u32,
    /// Rendered explanation for the checklist.
    snippet: String,
}

/// Optional classifiers a compliance finding may carry.
#[derive(Debug, Clone, Default)]
struct SiteClassifier {
    /// Foreign error type named at this site, if any.
    foreign_error_type: Option<String>,
    /// Internal error constructor used at this site, if any.
    internal_constructor: Option<String>,
}

impl SiteClassifier {
    /// Only the foreign error type is known.
    #[instrument(level = "debug")]
    fn foreign(foreign_error_type: Option<String>) -> Self {
        Self {
            foreign_error_type,
            internal_constructor: None,
        }
    }

    /// The internal constructor, and optionally the foreign type it wraps.
    #[instrument(level = "debug")]
    fn constructor(foreign_error_type: Option<String>, name: String) -> Self {
        Self {
            foreign_error_type,
            internal_constructor: Some(name),
        }
    }
}

impl Catalog {
    #[instrument(level = "debug", skip(self, field_strategy))]
    pub(super) fn into_findings(
        self,
        kind_fanout_max_call_sites: usize,
        field_strategy: ErrorFieldStrategy,
    ) -> CordialResult<Vec<InternalErrorComplianceFinding>> {
        let mut findings = Vec::new();
        self.emit_parent_findings(field_strategy, &mut findings)?;
        self.emit_kind_variant_findings(&mut findings)?;
        self.emit_kind_fanout_findings(kind_fanout_max_call_sites, &mut findings)?;
        self.emit_native_source_findings(field_strategy, &mut findings)?;
        self.emit_field_strategy_findings(field_strategy, &mut findings)?;
        self.emit_parent_track_caller(&mut findings)?;
        self.emit_map_err_track_caller(&mut findings)?;
        Ok(findings)
    }

    #[instrument(level = "debug", skip(self, field_strategy, findings))]
    fn emit_parent_findings(
        &self,
        field_strategy: ErrorFieldStrategy,
        findings: &mut Vec<InternalErrorComplianceFinding>,
    ) -> CordialResult<()> {
        let parents = self.root_parents();
        self.emit_missing_parent_findings(&parents, field_strategy, findings)?;
        self.emit_parent_storage_findings(&parents, field_strategy, findings)?;

        if parents.len() > 1 {
            for extra in parents.iter().skip(1) {
                findings.push(self.finding(
                    InternalErrorComplianceId::ArchParent001,
                    FindingSite::new(
                        extra.type_path().clone(),
                        extra.file().clone(),
                        extra.line(),
                        format!(
                            "{} — extra parent; one error type holds the umbrella Kind",
                            extra.snippet()
                        ),
                    ),
                    SiteClassifier::default(),
                )?);
            }
        }

        for item in self.structs().values() {
            if !self.impls_error(item.ident()) {
                continue;
            }
            if item.kind_unboxed_of().is_some()
                && item.kind_box_of().is_none()
                && item.kind_borrowed_of().is_none()
            {
                findings.push(self.finding(
                    InternalErrorComplianceId::ArchKindBox001,
                    FindingSite::new(
                        item.type_path().clone(),
                        item.file().clone(),
                        item.line(),
                        format!(
                            "{} — `kind` must be {}",
                            item.snippet(),
                            match field_strategy {
                                ErrorFieldStrategy::Owned => format!(
                                    "boxed (`Box<{}>`)",
                                    item.kind_unboxed_of().as_deref().unwrap_or("Kind")
                                ),
                                ErrorFieldStrategy::Borrowed => format!(
                                    "borrowed (`&'a {}`)",
                                    item.kind_unboxed_of().as_deref().unwrap_or("Kind")
                                ),
                            }
                        ),
                    ),
                    SiteClassifier::default(),
                )?);
            }
        }
        Ok(())
    }

    #[instrument(level = "debug", skip(self, parents, field_strategy, findings))]
    fn emit_missing_parent_findings(
        &self,
        parents: &[&StructInfo],
        field_strategy: ErrorFieldStrategy,
        findings: &mut Vec<InternalErrorComplianceFinding>,
    ) -> CordialResult<()> {
        let kind_enums: Vec<&EnumInfo> = self
            .enums()
            .values()
            .filter(|item| self.is_error_kind(item))
            .collect();
        let error_enums: Vec<&EnumInfo> = self
            .enums()
            .values()
            .filter(|item| self.impls_error(item.ident()) && Self::is_error_enum_name(item.ident()))
            .collect();

        if parents.is_empty() {
            if let Some(kind) = kind_enums.first() {
                findings.push(self.finding(
                    InternalErrorComplianceId::ArchParent001,
                    FindingSite::new(
                        kind.type_path().clone(),
                        kind.file().clone(),
                        kind.line(),
                        format!(
                            "{} — Kind must be held in a parent error (`kind: {}`)",
                            kind.snippet(),
                            match field_strategy {
                                ErrorFieldStrategy::Owned => format!("Box<{}>", kind.ident()),
                                ErrorFieldStrategy::Borrowed => format!("&'a {}", kind.ident()),
                            }
                        ),
                    ),
                    SiteClassifier::default(),
                )?);
            } else if let Some(error_enum) = error_enums.first() {
                findings.push(self.finding(
                    InternalErrorComplianceId::ArchParent001,
                    FindingSite::new(
                        error_enum.type_path().clone(),
                        error_enum.file().clone(),
                        error_enum.line(),
                        format!(
                            "{} — parent error is a struct holding `Kind`, not an error enum",
                            error_enum.snippet()
                        ),
                    ),
                    SiteClassifier::default(),
                )?);
            } else if let Some(source) = self
                .native_source_idents()
                .iter()
                .next()
                .and_then(|ident| self.structs().get(ident))
            {
                findings.push(self.finding(
                    InternalErrorComplianceId::ArchParent001,
                    FindingSite::new(
                        source.type_path().clone(),
                        source.file().clone(),
                        source.line(),
                        format!(
                            "native source without a parent error holding a Kind as {}",
                            match field_strategy {
                                ErrorFieldStrategy::Owned => "Box<Kind>",
                                ErrorFieldStrategy::Borrowed => "&'a Kind",
                            }
                        ),
                    ),
                    SiteClassifier::foreign(source.foreign_source().clone()),
                )?);
            }
        }

        Ok(())
    }

    #[instrument(level = "debug", skip(self, parents, field_strategy, findings))]
    fn emit_parent_storage_findings(
        &self,
        parents: &[&StructInfo],
        field_strategy: ErrorFieldStrategy,
        findings: &mut Vec<InternalErrorComplianceFinding>,
    ) -> CordialResult<()> {
        for parent in parents {
            let expected_kind = match field_strategy {
                ErrorFieldStrategy::Owned => format!(
                    "Box<{}>",
                    parent
                        .kind_borrowed_of()
                        .as_deref()
                        .or(parent.kind_unboxed_of().as_deref())
                        .unwrap_or("Kind")
                ),
                ErrorFieldStrategy::Borrowed => format!(
                    "&'a {}",
                    parent
                        .kind_box_of()
                        .as_deref()
                        .or(parent.kind_unboxed_of().as_deref())
                        .unwrap_or("Kind")
                ),
            };
            let wrong_storage = match field_strategy {
                ErrorFieldStrategy::Owned => parent.kind_borrowed_of().is_some(),
                ErrorFieldStrategy::Borrowed => parent.kind_box_of().is_some(),
            };
            if wrong_storage {
                findings.push(self.finding(
                    InternalErrorComplianceId::ArchKindBox001,
                    FindingSite::new(
                        parent.type_path().clone(),
                        parent.file().clone(),
                        parent.line(),
                        format!("{} — `kind` must be `{expected_kind}`", parent.snippet()),
                    ),
                    SiteClassifier::default(),
                )?);
            }
            if let Some(unboxed) = &parent.kind_unboxed_of() {
                findings.push(self.finding(
                    InternalErrorComplianceId::ArchKindBox001,
                    FindingSite::new(
                        parent.type_path().clone(),
                        parent.file().clone(),
                        parent.line(),
                        format!(
                            "{} — `kind` must be {}",
                            parent.snippet(),
                            match field_strategy {
                                ErrorFieldStrategy::Owned => format!("Box<{unboxed}>"),
                                ErrorFieldStrategy::Borrowed => format!("&'a {unboxed}"),
                            }
                        ),
                    ),
                    SiteClassifier::default(),
                )?);
            }
            if let Some(kind_ident) = parent
                .kind_box_of()
                .as_ref()
                .or(parent.kind_borrowed_of().as_ref())
                && !Self::is_kind_name(kind_ident)
            {
                findings.push(self.finding(
                    InternalErrorComplianceId::ArchParent001,
                    FindingSite::new(
                        parent.type_path().clone(),
                        parent.file().clone(),
                        parent.line(),
                        format!(
                            "{} — `kind` type `{kind_ident}` must be a `*Kind` enum",
                            parent.snippet()
                        ),
                    ),
                    SiteClassifier::default(),
                )?);
            }
        }

        Ok(())
    }

    #[instrument(level = "debug", skip(self, field_strategy, findings))]
    fn emit_native_source_findings(
        &self,
        field_strategy: ErrorFieldStrategy,
        findings: &mut Vec<InternalErrorComplianceFinding>,
    ) -> CordialResult<()> {
        let payloads = self.kind_payload_idents();
        let kind_idents: BTreeSet<String> = self
            .enums()
            .values()
            .filter(|item| self.is_error_kind(item))
            .map(|item| item.ident().clone())
            .collect();

        for ident in self.native_source_idents() {
            let Some(item) = self.structs().get(&ident) else {
                continue;
            };
            if !payloads.contains(&ident) && !kind_idents.is_empty() {
                findings.push(self.finding(
                    InternalErrorComplianceId::ArchOrphanSource001,
                    FindingSite::new(
                        item.type_path().clone(),
                        item.file().clone(),
                        item.line(),
                        format!(
                            "{} — native source must appear as a Kind variant",
                            item.snippet()
                        ),
                    ),
                    SiteClassifier::foreign(item.foreign_source().clone()),
                )?);
            }

            let nested = item.kind_box_of().is_some() || item.kind_borrowed_of().is_some();
            let foreign = item.foreign_source().is_some();
            if foreign && !item.has_source_field() {
                findings.push(self.finding(
                    InternalErrorComplianceId::SourceShape001,
                    FindingSite::new(
                        item.type_path().clone(),
                        item.file().clone(),
                        item.line(),
                        format!(
                            "{} — foreign error must live in a `source` field",
                            item.snippet()
                        ),
                    ),
                    SiteClassifier::foreign(item.foreign_source().clone()),
                )?);
            }
            if item.has_location() {
                findings.push(self.finding(
                    InternalErrorComplianceId::SourceShape001,
                    FindingSite::new(
                        item.type_path().clone(),
                        item.file().clone(),
                        item.line(),
                        format!(
                            "{} — {}; do not store `&'static Location`",
                            item.snippet(), match field_strategy {
                                ErrorFieldStrategy::Owned => "copy owned `file` and `line` from `Location::caller()`",
                                ErrorFieldStrategy::Borrowed => "store `file: &'static str` and copy `line` from `Location::caller()`",
                            }
                        ),
                    ),
                    SiteClassifier::foreign(item.foreign_source().clone()),
                )?);
            } else if !item.location_complete() {
                findings.push(self.finding(
                    InternalErrorComplianceId::SourceShape001,
                    FindingSite::new(
                        item.type_path().clone(),
                        item.file().clone(),
                        item.line(),
                        format!(
                            "{} — native source needs {} from `Location::caller()`",
                            item.snippet(),
                            match field_strategy {
                                ErrorFieldStrategy::Owned => "owned `file` and copied `line`",
                                ErrorFieldStrategy::Borrowed =>
                                    "`file: &'static str` and copied `line`",
                            }
                        ),
                    ),
                    SiteClassifier::foreign(item.foreign_source().clone()),
                )?);
            }
            self.emit_track_caller(findings, item, foreign, nested)?;
        }
        Ok(())
    }

    #[instrument(level = "debug", skip(self, rule_id, site, classifier))]
    fn finding(
        &self,
        rule_id: InternalErrorComplianceId,
        site: FindingSite,
        classifier: SiteClassifier,
    ) -> CordialResult<InternalErrorComplianceFinding> {
        InternalErrorComplianceFinding::builder()
            .crate_name(self.crate_name().clone())
            .rule_id(rule_id)
            .context(site.context)
            .file(site.file)
            .line(site.line)
            .snippet(site.snippet)
            .foreign_error_type(classifier.foreign_error_type)
            .internal_constructor(classifier.internal_constructor)
            .build()
    }
}

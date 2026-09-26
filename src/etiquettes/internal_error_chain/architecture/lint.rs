//! Parent / Kind / native-source findings from a completed catalog.

use std::collections::BTreeSet;
use std::path::PathBuf;

use super::super::types::{InternalErrorComplianceFinding, InternalErrorComplianceId};
use super::catalog::{Catalog, EnumInfo};
use crate::error::CordialResult;

use tracing::instrument;

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
    #[instrument(level = "debug", skip(self))]
    pub(super) fn into_findings(self) -> CordialResult<Vec<InternalErrorComplianceFinding>> {
        let mut findings = Vec::new();
        self.emit_parent_findings(&mut findings)?;
        self.emit_kind_variant_findings(&mut findings)?;
        self.emit_native_source_findings(&mut findings)?;
        self.emit_parent_track_caller(&mut findings)?;
        Ok(findings)
    }

    #[instrument(level = "debug", skip(self, findings))]
    fn emit_parent_findings(
        &self,
        findings: &mut Vec<InternalErrorComplianceFinding>,
    ) -> CordialResult<()> {
        let parents = self.root_parents();
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
                            "{} — Kind must be boxed in a parent error (`kind: Box<{}>`)",
                            kind.snippet(),
                            kind.ident()
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
                            "{} — parent error is a struct boxing `Kind`, not an error enum",
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
                        "native source without a parent error boxing a Kind".to_string(),
                    ),
                    SiteClassifier::foreign(source.foreign_source().clone()),
                )?);
            }
        }

        for parent in &parents {
            if let Some(unboxed) = &parent.kind_unboxed_of() {
                findings.push(self.finding(
                    InternalErrorComplianceId::ArchKindBox001,
                    FindingSite::new(
                        parent.type_path().clone(),
                        parent.file().clone(),
                        parent.line(),
                        format!("{} — `kind` must be `Box<{unboxed}>`", parent.snippet()),
                    ),
                    SiteClassifier::default(),
                )?);
            }
            if let Some(kind_ident) = &parent.kind_box_of()
                && !Self::is_kind_name(kind_ident)
            {
                findings.push(self.finding(
                    InternalErrorComplianceId::ArchParent001,
                    FindingSite::new(
                        parent.type_path().clone(),
                        parent.file().clone(),
                        parent.line(),
                        format!(
                            "{} — boxed type `{kind_ident}` must be a `*Kind` enum",
                            parent.snippet()
                        ),
                    ),
                    SiteClassifier::default(),
                )?);
            }
        }

        if parents.len() > 1 {
            for extra in parents.iter().skip(1) {
                findings.push(self.finding(
                    InternalErrorComplianceId::ArchParent001,
                    FindingSite::new(
                        extra.type_path().clone(),
                        extra.file().clone(),
                        extra.line(),
                        format!(
                            "{} — extra parent; one error type boxes the umbrella Kind",
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
            if item.kind_unboxed_of().is_some() && item.kind_box_of().is_none() {
                findings.push(self.finding(
                    InternalErrorComplianceId::ArchKindBox001,
                    FindingSite::new(
                        item.type_path().clone(),
                        item.file().clone(),
                        item.line(),
                        format!(
                            "{} — `kind` must be boxed (`Box<{}>`)",
                            item.snippet(),
                            item.kind_unboxed_of().as_deref().unwrap_or("Kind")
                        ),
                    ),
                    SiteClassifier::default(),
                )?);
            }
        }
        Ok(())
    }

    #[instrument(level = "debug", skip(self, findings))]
    fn emit_native_source_findings(
        &self,
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

            let nested = item.kind_box_of().is_some();
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
                            "{} — copy owned `file` and `line` from `Location::caller()`; \
                         do not store `&'static Location`",
                            item.snippet()
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
                            "{} — native source needs owned `file`+`line` copied from \
                         `Location::caller()`",
                            item.snippet()
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

//! Parent / Kind / native-source findings from a completed catalog.

use std::collections::BTreeSet;
use std::path::PathBuf;

use super::super::source_shape::type_labels_match;
use super::super::type_graph::is_foreign_type_label;
use super::super::types::{InternalErrorComplianceFinding, InternalErrorComplianceId};
use super::catalog::{Catalog, ConstructorRec, EnumInfo, StructInfo, VariantInfo};
use crate::error::CordialResult;

use tracing::instrument;

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
    fn foreign(foreign_error_type: Option<String>) -> Self {
        Self {
            foreign_error_type,
            internal_constructor: None,
        }
    }

    /// The internal constructor, and optionally the foreign type it wraps.
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
            .enums
            .values()
            .filter(|item| self.is_error_kind(item))
            .collect();
        let error_enums: Vec<&EnumInfo> = self
            .enums
            .values()
            .filter(|item| self.impls_error(&item.ident) && Self::is_error_enum_name(&item.ident))
            .collect();

        if parents.is_empty() {
            if let Some(kind) = kind_enums.first() {
                findings.push(self.finding(
                    InternalErrorComplianceId::ArchParent001,
                    FindingSite::new(
                        kind.type_path.clone(),
                        kind.file.clone(),
                        kind.line,
                        format!(
                            "{} — Kind must be boxed in a parent error (`kind: Box<{}>`)",
                            kind.snippet, kind.ident
                        ),
                    ),
                    SiteClassifier::default(),
                )?);
            } else if let Some(error_enum) = error_enums.first() {
                findings.push(self.finding(
                    InternalErrorComplianceId::ArchParent001,
                    FindingSite::new(
                        error_enum.type_path.clone(),
                        error_enum.file.clone(),
                        error_enum.line,
                        format!(
                            "{} — parent error is a struct boxing `Kind`, not an error enum",
                            error_enum.snippet
                        ),
                    ),
                    SiteClassifier::default(),
                )?);
            } else if let Some(source) = self
                .native_source_idents()
                .iter()
                .next()
                .and_then(|ident| self.structs.get(ident))
            {
                findings.push(self.finding(
                    InternalErrorComplianceId::ArchParent001,
                    FindingSite::new(
                        source.type_path.clone(),
                        source.file.clone(),
                        source.line,
                        "native source without a parent error boxing a Kind".to_string(),
                    ),
                    SiteClassifier::foreign(source.foreign_source.clone()),
                )?);
            }
        }

        for parent in &parents {
            if let Some(unboxed) = &parent.kind_unboxed_of {
                findings.push(self.finding(
                    InternalErrorComplianceId::ArchKindBox001,
                    FindingSite::new(
                        parent.type_path.clone(),
                        parent.file.clone(),
                        parent.line,
                        format!("{} — `kind` must be `Box<{unboxed}>`", parent.snippet),
                    ),
                    SiteClassifier::default(),
                )?);
            }
            if let Some(kind_ident) = &parent.kind_box_of
                && !Self::is_kind_name(kind_ident)
            {
                findings.push(self.finding(
                    InternalErrorComplianceId::ArchParent001,
                    FindingSite::new(
                        parent.type_path.clone(),
                        parent.file.clone(),
                        parent.line,
                        format!(
                            "{} — boxed type `{kind_ident}` must be a `*Kind` enum",
                            parent.snippet
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
                        extra.type_path.clone(),
                        extra.file.clone(),
                        extra.line,
                        format!(
                            "{} — extra parent; one error type boxes the umbrella Kind",
                            extra.snippet
                        ),
                    ),
                    SiteClassifier::default(),
                )?);
            }
        }

        for item in self.structs.values() {
            if !self.impls_error(&item.ident) {
                continue;
            }
            if item.kind_unboxed_of.is_some() && item.kind_box_of.is_none() {
                findings.push(self.finding(
                    InternalErrorComplianceId::ArchKindBox001,
                    FindingSite::new(
                        item.type_path.clone(),
                        item.file.clone(),
                        item.line,
                        format!(
                            "{} — `kind` must be boxed (`Box<{}>`)",
                            item.snippet,
                            item.kind_unboxed_of.as_deref().unwrap_or("Kind")
                        ),
                    ),
                    SiteClassifier::default(),
                )?);
            }
        }
        Ok(())
    }

    #[instrument(level = "debug", skip(self, findings))]
    fn emit_kind_variant_findings(
        &self,
        findings: &mut Vec<InternalErrorComplianceFinding>,
    ) -> CordialResult<()> {
        let parent_idents: BTreeSet<String> = self
            .root_parents()
            .into_iter()
            .map(|item| item.ident.clone())
            .collect();
        for item in self.enums.values() {
            let lint = self.is_error_kind(item)
                || (self.impls_error(&item.ident) && Self::is_error_enum_name(&item.ident));
            if !lint {
                continue;
            }
            for variant in &item.variants {
                self.lint_variant_payloads(findings, item, variant, &parent_idents)?;
            }
        }
        Ok(())
    }

    #[instrument(level = "debug", skip(self, findings, item, variant, parent_idents))]
    fn lint_variant_payloads(
        &self,
        findings: &mut Vec<InternalErrorComplianceFinding>,
        item: &EnumInfo,
        variant: &VariantInfo,
        parent_idents: &BTreeSet<String>,
    ) -> CordialResult<()> {
        let context = format!("{}::{}", item.type_path, variant.name);
        if variant.payloads.len() != 1 {
            findings.push(self.finding(
                InternalErrorComplianceId::ArchKindVariant001,
                FindingSite::new(
                    context,
                    item.file.clone(),
                    variant.line,
                    format!(
                        "{} — Kind variant must be a 1-tuple native source",
                        variant.snippet
                    ),
                ),
                SiteClassifier::default(),
            )?);
            return Ok(());
        }
        let payload = &variant.payloads[0];
        let payload_ident = Self::last_ident(payload).to_string();
        if is_foreign_type_label(payload) {
            findings.push(self.finding(
                InternalErrorComplianceId::ArchKindVariant001,
                FindingSite::new(
                    context,
                    item.file.clone(),
                    variant.line,
                    format!(
                        "{} — wrap the foreign error in a native source, not `{payload}`",
                        variant.snippet
                    ),
                ),
                SiteClassifier::foreign(Some(payload.clone())),
            )?);
            return Ok(());
        }
        if payload_ident == "String" {
            findings.push(self.finding(
                InternalErrorComplianceId::ArchKindVariant001,
                FindingSite::new(
                    context,
                    item.file.clone(),
                    variant.line,
                    format!("{} — String is not a native source", variant.snippet),
                ),
                SiteClassifier::default(),
            )?);
            return Ok(());
        }
        if Self::is_kind_name(&payload_ident) || parent_idents.contains(&payload_ident) {
            findings.push(self.finding(
                InternalErrorComplianceId::ArchKindVariant001,
                FindingSite::new(
                    context,
                    item.file.clone(),
                    variant.line,
                    format!(
                        "{} — variant must hold a native source, not `{payload_ident}`",
                        variant.snippet
                    ),
                ),
                SiteClassifier::default(),
            )?);
            return Ok(());
        }
        if !self.structs.contains_key(&payload_ident) {
            findings.push(self.finding(
                InternalErrorComplianceId::ArchKindVariant001,
                FindingSite::new(
                    context,
                    item.file.clone(),
                    variant.line,
                    format!(
                        "{} — wrap `{payload_ident}` in a native source",
                        variant.snippet
                    ),
                ),
                SiteClassifier::default(),
            )?);
        } else if !self.impls_error(&payload_ident) {
            findings.push(self.finding(
                InternalErrorComplianceId::ArchKindVariant001,
                FindingSite::new(
                    context,
                    item.file.clone(),
                    variant.line,
                    format!(
                        "{} — `{payload_ident}` must implement Error",
                        variant.snippet
                    ),
                ),
                SiteClassifier::default(),
            )?);
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
            .enums
            .values()
            .filter(|item| self.is_error_kind(item))
            .map(|item| item.ident.clone())
            .collect();

        for ident in self.native_source_idents() {
            let Some(item) = self.structs.get(&ident) else {
                continue;
            };
            if !payloads.contains(&ident) && !kind_idents.is_empty() {
                findings.push(self.finding(
                    InternalErrorComplianceId::ArchOrphanSource001,
                    FindingSite::new(
                        item.type_path.clone(),
                        item.file.clone(),
                        item.line,
                        format!(
                            "{} — native source must appear as a Kind variant",
                            item.snippet
                        ),
                    ),
                    SiteClassifier::foreign(item.foreign_source.clone()),
                )?);
            }

            let nested = item.kind_box_of.is_some();
            let foreign = item.foreign_source.is_some();
            if foreign && !item.has_source_field {
                findings.push(self.finding(
                    InternalErrorComplianceId::SourceShape001,
                    FindingSite::new(
                        item.type_path.clone(),
                        item.file.clone(),
                        item.line,
                        format!(
                            "{} — foreign error must live in a `source` field",
                            item.snippet
                        ),
                    ),
                    SiteClassifier::foreign(item.foreign_source.clone()),
                )?);
            }
            if item.has_location {
                findings.push(self.finding(
                    InternalErrorComplianceId::SourceShape001,
                    FindingSite::new(
                        item.type_path.clone(),
                        item.file.clone(),
                        item.line,
                        format!(
                            "{} — copy owned `file` and `line` from `Location::caller()`; \
                         do not store `&'static Location`",
                            item.snippet
                        ),
                    ),
                    SiteClassifier::foreign(item.foreign_source.clone()),
                )?);
            } else if !item.location_complete() {
                findings.push(self.finding(
                    InternalErrorComplianceId::SourceShape001,
                    FindingSite::new(
                        item.type_path.clone(),
                        item.file.clone(),
                        item.line,
                        format!(
                            "{} — native source needs owned `file`+`line` copied from \
                         `Location::caller()`",
                            item.snippet
                        ),
                    ),
                    SiteClassifier::foreign(item.foreign_source.clone()),
                )?);
            }
            self.emit_track_caller(findings, item, foreign, nested)?;
        }
        Ok(())
    }

    #[instrument(level = "debug", skip(self, findings, item))]
    fn emit_track_caller(
        &self,
        findings: &mut Vec<InternalErrorComplianceFinding>,
        item: &StructInfo,
        foreign: bool,
        nested: bool,
    ) -> CordialResult<()> {
        let ctors: Vec<&ConstructorRec> = self
            .constructors
            .iter()
            .filter(|ctor| ctor.self_ident == item.ident)
            .collect();
        let news: Vec<&ConstructorRec> = ctors
            .iter()
            .copied()
            .filter(|ctor| ctor.name == "new")
            .collect();

        if news.is_empty() {
            findings.push(self.finding(
                InternalErrorComplianceId::SourceTrackCaller001,
                FindingSite::new(
                    item.type_path.clone(),
                    item.file.clone(),
                    item.line,
                    format!(
                        "{} — write `#[track_caller] fn new` that calls `Location::caller()`",
                        item.snippet
                    ),
                ),
                SiteClassifier::foreign(item.foreign_source.clone()),
            )?);
        }
        for ctor in news {
            if !ctor.has_track_caller {
                findings.push(self.finding(
                    InternalErrorComplianceId::SourceTrackCaller001,
                    FindingSite::new(
                        item.type_path.clone(),
                        item.file.clone(),
                        ctor.line,
                        format!("{}::new must be `#[track_caller]`", item.ident),
                    ),
                    SiteClassifier::constructor(item.foreign_source.clone(), ctor.name.clone()),
                )?);
            }
            if !ctor.captures_location {
                findings.push(self.finding(
                    InternalErrorComplianceId::SourceTrackCaller001,
                    FindingSite::new(
                        item.type_path.clone(),
                        item.file.clone(),
                        ctor.line,
                        format!(
                            "{}::new must call `Location::caller()` in its body",
                            item.ident
                        ),
                    ),
                    SiteClassifier::constructor(item.foreign_source.clone(), ctor.name.clone()),
                )?);
            }
            if ctor.takes_location_arg {
                findings.push(self.finding(
                    InternalErrorComplianceId::SourceTrackCaller001,
                    FindingSite::new(
                        item.type_path.clone(),
                        item.file.clone(),
                        ctor.line,
                        format!(
                        "{}::new must not take file/line/location; get them from `Location::caller()`",
                        item.ident
                    ),
                    ),
                    SiteClassifier::constructor(item.foreign_source.clone(), ctor.name.clone()),
                )?);
            }
        }

        for ctor in ctors {
            if ctor.name == "new" {
                continue;
            }
            let relevant = ctor.from_trait
                || (foreign
                    && item.foreign_source.as_ref().is_some_and(|foreign_ty| {
                        ctor.input_labels
                            .iter()
                            .any(|label| type_labels_match(label, foreign_ty))
                    }))
                || (nested
                    && item.kind_box_of.as_ref().is_some_and(|kind| {
                        ctor.input_labels
                            .iter()
                            .any(|label| Catalog::last_ident(label) == kind)
                    }));
            if !relevant {
                continue;
            }
            if !ctor.has_track_caller {
                findings.push(self.finding(
                    InternalErrorComplianceId::SourceTrackCaller001,
                    FindingSite::new(
                        item.type_path.clone(),
                        item.file.clone(),
                        ctor.line,
                        format!(
                            "{}::{} must be `#[track_caller]` so it can delegate to `new`",
                            item.ident, ctor.name
                        ),
                    ),
                    SiteClassifier::constructor(item.foreign_source.clone(), ctor.name.clone()),
                )?);
            }
            if ctor.takes_location_arg {
                findings.push(self.finding(
                    InternalErrorComplianceId::SourceTrackCaller001,
                    FindingSite::new(
                        item.type_path.clone(),
                        item.file.clone(),
                        ctor.line,
                        format!(
                            "{}::{} must not take file/line/location; capture them in `new`",
                            item.ident, ctor.name
                        ),
                    ),
                    SiteClassifier::constructor(item.foreign_source.clone(), ctor.name.clone()),
                )?);
            }
        }
        Ok(())
    }

    #[instrument(level = "debug", skip(self, findings))]
    fn emit_parent_track_caller(
        &self,
        findings: &mut Vec<InternalErrorComplianceFinding>,
    ) -> CordialResult<()> {
        for parent in self.root_parents() {
            for ctor in self
                .constructors
                .iter()
                .filter(|ctor| ctor.self_ident == parent.ident)
            {
                if ctor.has_track_caller {
                    continue;
                }
                findings.push(self.finding(
                    InternalErrorComplianceId::SourceTrackCaller001,
                    FindingSite::new(
                        parent.type_path.clone(),
                        parent.file.clone(),
                        ctor.line,
                        format!(
                        "{}::{} must be `#[track_caller]` so native-source `new` sees the call site",
                        parent.ident, ctor.name
                    ),
                    ),
                    SiteClassifier::constructor(None, ctor.name.clone()),
                )?);
            }
        }
        Ok(())
    }

    #[instrument(level = "trace", skip(self, rule_id, site))]
    fn finding(
        &self,
        rule_id: InternalErrorComplianceId,
        site: FindingSite,
        classifier: SiteClassifier,
    ) -> CordialResult<InternalErrorComplianceFinding> {
        InternalErrorComplianceFinding::builder()
            .crate_name(self.crate_name.clone())
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

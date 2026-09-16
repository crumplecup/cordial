//! Track-caller constructor findings for internal error architecture.

use crate::error::CordialResult;
use crate::etiquettes::internal_error_chain::source_shape::type_labels_match;
use crate::etiquettes::internal_error_chain::types::{
    InternalErrorComplianceFinding, InternalErrorComplianceId,
};

use tracing::instrument;

use super::super::catalog::{Catalog, ConstructorRec, StructInfo};
use super::{FindingSite, SiteClassifier};

impl Catalog {
    #[instrument(level = "debug", skip(self, findings, item))]
    pub(super) fn emit_track_caller(
        &self,
        findings: &mut Vec<InternalErrorComplianceFinding>,
        item: &StructInfo,
        foreign: bool,
        nested: bool,
    ) -> CordialResult<()> {
        let ctors = self.constructors_for(item);
        let news: Vec<&ConstructorRec> = ctors
            .iter()
            .copied()
            .filter(|ctor| ctor.name() == "new")
            .collect();

        if news.is_empty() {
            findings.push(self.missing_new_finding(item)?);
        }
        for ctor in news {
            self.emit_new_track_caller_findings(findings, item, ctor)?;
        }
        for ctor in ctors {
            if self.delegating_ctor_needs_track_caller(item, ctor, foreign, nested) {
                self.emit_delegating_track_caller_findings(findings, item, ctor)?;
            }
        }
        Ok(())
    }

    #[instrument(level = "debug", skip(self, findings))]
    pub(super) fn emit_parent_track_caller(
        &self,
        findings: &mut Vec<InternalErrorComplianceFinding>,
    ) -> CordialResult<()> {
        for parent in self.root_parents() {
            for ctor in self
                .constructors()
                .iter()
                .filter(|ctor| ctor.self_ident() == parent.ident())
            {
                if ctor.has_track_caller() {
                    continue;
                }
                findings.push(self.finding(
                    InternalErrorComplianceId::SourceTrackCaller001,
                    FindingSite::new(
                        parent.type_path().clone(),
                        parent.file().clone(),
                        ctor.line(),
                        format!(
                            "{}::{} must be `#[track_caller]` so native-source `new` sees the call site",
                            parent.ident(),
                            ctor.name()
                        ),
                    ),
                    SiteClassifier::constructor(None, ctor.name().clone()),
                )?);
            }
        }
        Ok(())
    }

    #[instrument(level = "trace", skip(self, item))]
    fn constructors_for(&self, item: &StructInfo) -> Vec<&ConstructorRec> {
        self.constructors()
            .iter()
            .filter(|ctor| ctor.self_ident() == item.ident())
            .collect()
    }

    #[instrument(level = "debug", skip(self, item))]
    fn missing_new_finding(
        &self,
        item: &StructInfo,
    ) -> CordialResult<InternalErrorComplianceFinding> {
        self.finding(
            InternalErrorComplianceId::SourceTrackCaller001,
            FindingSite::new(
                item.type_path().clone(),
                item.file().clone(),
                item.line(),
                format!(
                    "{} — write `#[track_caller] fn new` that calls `Location::caller()`",
                    item.snippet()
                ),
            ),
            SiteClassifier::foreign(item.foreign_source().clone()),
        )
    }

    #[instrument(level = "debug", skip(self, findings, item, ctor))]
    fn emit_new_track_caller_findings(
        &self,
        findings: &mut Vec<InternalErrorComplianceFinding>,
        item: &StructInfo,
        ctor: &ConstructorRec,
    ) -> CordialResult<()> {
        if !ctor.has_track_caller() {
            findings.push(self.finding(
                InternalErrorComplianceId::SourceTrackCaller001,
                FindingSite::new(
                    item.type_path().clone(),
                    item.file().clone(),
                    ctor.line(),
                    format!("{}::new must be `#[track_caller]`", item.ident()),
                ),
                SiteClassifier::constructor(item.foreign_source().clone(), ctor.name().clone()),
            )?);
        }
        if !ctor.captures_location() {
            findings.push(self.finding(
                InternalErrorComplianceId::SourceTrackCaller001,
                FindingSite::new(
                    item.type_path().clone(),
                    item.file().clone(),
                    ctor.line(),
                    format!(
                        "{}::new must call `Location::caller()` in its body",
                        item.ident()
                    ),
                ),
                SiteClassifier::constructor(item.foreign_source().clone(), ctor.name().clone()),
            )?);
        }
        if ctor.takes_location_arg() {
            findings.push(self.finding(
                InternalErrorComplianceId::SourceTrackCaller001,
                FindingSite::new(
                    item.type_path().clone(),
                    item.file().clone(),
                    ctor.line(),
                    format!(
                        "{}::new must not take file/line/location; get them from `Location::caller()`",
                        item.ident()
                    ),
                ),
                SiteClassifier::constructor(item.foreign_source().clone(), ctor.name().clone()),
            )?);
        }
        Ok(())
    }

    #[instrument(level = "trace", skip(self, item, ctor))]
    fn delegating_ctor_needs_track_caller(
        &self,
        item: &StructInfo,
        ctor: &ConstructorRec,
        foreign: bool,
        nested: bool,
    ) -> bool {
        if ctor.name() == "new" {
            return false;
        }
        ctor.from_trait()
            || (foreign
                && item.foreign_source().as_ref().is_some_and(|foreign_ty| {
                    ctor.input_labels()
                        .iter()
                        .any(|label| type_labels_match(label, foreign_ty))
                }))
            || (nested
                && item.kind_box_of().as_ref().is_some_and(|kind| {
                    ctor.input_labels()
                        .iter()
                        .any(|label| Catalog::last_ident(label) == kind)
                }))
    }

    #[instrument(level = "debug", skip(self, findings, item, ctor))]
    fn emit_delegating_track_caller_findings(
        &self,
        findings: &mut Vec<InternalErrorComplianceFinding>,
        item: &StructInfo,
        ctor: &ConstructorRec,
    ) -> CordialResult<()> {
        if !ctor.has_track_caller() {
            findings.push(self.finding(
                InternalErrorComplianceId::SourceTrackCaller001,
                FindingSite::new(
                    item.type_path().clone(),
                    item.file().clone(),
                    ctor.line(),
                    format!(
                        "{}::{} must be `#[track_caller]` so it can delegate to `new`",
                        item.ident(),
                        ctor.name()
                    ),
                ),
                SiteClassifier::constructor(item.foreign_source().clone(), ctor.name().clone()),
            )?);
        }
        if ctor.takes_location_arg() {
            findings.push(self.finding(
                InternalErrorComplianceId::SourceTrackCaller001,
                FindingSite::new(
                    item.type_path().clone(),
                    item.file().clone(),
                    ctor.line(),
                    format!(
                        "{}::{} must not take file/line/location; capture them in `new`",
                        item.ident(),
                        ctor.name()
                    ),
                ),
                SiteClassifier::constructor(item.foreign_source().clone(), ctor.name().clone()),
            )?);
        }
        Ok(())
    }
}

//! Finding/Rule for amenable ext (jiff) registry coverage — the
//! `amenable_ext` counterpart of `etiquettes::framework_std::amenable`,
//! which does the identical round trip for `amenable_std`.

use crate::error::CordialResult;
use crate::framework_std::{
    AmenableStdEntry, AmenableStdGapEntry, AmenableStdReport, AmenableStdStatus,
};
use crate::objects::{Disposition, Finding, FindingSink, IrAnchor, NodeAnchor, Rule};

use super::AMENABLE_EXT_JIFF_CATEGORY;

use tracing::instrument;
#[derive(Debug, Clone, Copy)]
pub struct AmenableExtJiffRule;

impl Rule for AmenableExtJiffRule {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        "AMENABLE-EXT-JIFF-ROW"
    }

    #[instrument(level = "trace", skip(self))]
    fn category(&self) -> &str {
        AMENABLE_EXT_JIFF_CATEGORY
    }

    #[instrument(level = "trace", skip(self))]
    fn description(&self) -> &str {
        "Jiff inventory row assessed for amenable_ext registry coverage"
    }
}

#[derive(Debug, Clone, derive_builder::Builder)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub struct AmenableExtJiffRowFinding {
    rule: AmenableExtJiffRule,
    disposition: Disposition,
    anchor: NodeAnchor,
    source_crate: String,
    impl_crate: String,
    type_path: String,
    type_kind: String,
    is_generic: bool,
    status: AmenableStdStatus,
    evidence_link: bool,
    evidence_name: Option<String>,
    kani_witness: bool,
    creusot_witness: bool,
    verus_witness: bool,
    proof_test: bool,
    skip_reason: Option<String>,
    kani_excepted: bool,
    creusot_excepted: bool,
    verus_excepted: bool,
    missing_layers: String,
    action: String,
}

impl AmenableExtJiffRowFinding {
    /// Start a builder for this finding.
    #[instrument(level = "debug")]
    pub fn builder() -> AmenableExtJiffRowFindingBuilder {
        AmenableExtJiffRowFindingBuilder::default()
    }
}

impl Finding for AmenableExtJiffRowFinding {
    #[instrument(level = "trace", skip(self))]
    fn rule(&self) -> &dyn Rule {
        &self.rule
    }

    #[instrument(level = "trace", skip(self))]
    fn disposition(&self) -> Disposition {
        self.disposition
    }

    #[instrument(level = "trace", skip(self))]
    fn anchor(&self) -> &dyn IrAnchor {
        &self.anchor
    }

    #[instrument(level = "trace", skip(self, sink))]
    fn emit(&self, sink: &mut dyn FindingSink) {
        sink.field("source_crate", &self.source_crate);
        sink.field("impl_crate", &self.impl_crate);
        sink.field("type_path", &self.type_path);
        sink.field("type_kind", &self.type_kind);
        sink.field(
            "is_generic",
            if self.is_generic { &"true" } else { &"false" },
        );
        sink.field("status", &self.status.to_string());
        sink.field(
            "evidence_link",
            if self.evidence_link {
                &"true"
            } else {
                &"false"
            },
        );
        sink.field(
            "evidence_name",
            &self.evidence_name.as_deref().unwrap_or(""),
        );
        sink.field(
            "kani_witness",
            if self.kani_witness { &"true" } else { &"false" },
        );
        sink.field(
            "creusot_witness",
            if self.creusot_witness {
                &"true"
            } else {
                &"false"
            },
        );
        sink.field(
            "verus_witness",
            if self.verus_witness {
                &"true"
            } else {
                &"false"
            },
        );
        sink.field(
            "proof_test",
            if self.proof_test { &"true" } else { &"false" },
        );
        sink.field("skip_reason", &self.skip_reason.as_deref().unwrap_or(""));
        sink.field(
            "kani_excepted",
            if self.kani_excepted {
                &"true"
            } else {
                &"false"
            },
        );
        sink.field(
            "creusot_excepted",
            if self.creusot_excepted {
                &"true"
            } else {
                &"false"
            },
        );
        sink.field(
            "verus_excepted",
            if self.verus_excepted {
                &"true"
            } else {
                &"false"
            },
        );
        sink.field("missing_layers", &self.missing_layers);
        sink.field("action", &self.action);
    }
}

#[instrument(level = "debug", skip(status))]
pub fn amenable_ext_jiff_row_disposition(status: AmenableStdStatus) -> Disposition {
    match status {
        AmenableStdStatus::Missing | AmenableStdStatus::Partial => Disposition::Open,
        AmenableStdStatus::Skipped => Disposition::Suppressed,
        AmenableStdStatus::Complete => Disposition::Exemplar,
    }
}

#[instrument(level = "debug", skip(findings))]
pub fn amenable_ext_jiff_report_from_findings(
    findings: &[&dyn Finding],
    include_nightly: bool,
) -> CordialResult<Option<AmenableStdReport>> {
    let rows: Vec<_> = findings
        .iter()
        .filter(|finding| finding.rule().category() == AMENABLE_EXT_JIFF_CATEGORY)
        .collect();
    if rows.is_empty() {
        return Ok(None);
    }

    let mut entries = Vec::new();
    let mut complete_count = 0usize;
    let mut partial_count = 0usize;
    let mut missing_count = 0usize;
    let mut skipped_count = 0usize;
    let mut source_crate = String::new();
    let mut impl_crate = String::new();

    for finding in rows {
        let mut sink = crate::objects::MapFindingSink::default();
        finding.emit(&mut sink);
        let field = |name: &str| {
            sink.fields()
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.clone())
                .unwrap_or_default()
        };
        if source_crate.is_empty() {
            source_crate = field("source_crate");
            impl_crate = field("impl_crate");
        }
        let status = match field("status").as_str() {
            "Complete" => AmenableStdStatus::Complete,
            "Partial" => AmenableStdStatus::Partial,
            "Skipped" => AmenableStdStatus::Skipped,
            _ => AmenableStdStatus::Missing,
        };
        match status {
            AmenableStdStatus::Complete => complete_count += 1,
            AmenableStdStatus::Partial => partial_count += 1,
            AmenableStdStatus::Missing => missing_count += 1,
            AmenableStdStatus::Skipped => skipped_count += 1,
        }
        let evidence_name = {
            let name = field("evidence_name");
            if name.is_empty() { None } else { Some(name) }
        };
        let skip_reason = {
            let reason = field("skip_reason");
            if reason.is_empty() {
                None
            } else {
                Some(reason)
            }
        };
        entries.push(
            AmenableStdEntry::builder()
                .type_path(field("type_path"))
                .type_kind(field("type_kind"))
                .is_generic(field("is_generic") == "true")
                .evidence_link(field("evidence_link") == "true")
                .evidence_name(evidence_name)
                .kani_witness(field("kani_witness") == "true")
                .creusot_witness(field("creusot_witness") == "true")
                .verus_witness(field("verus_witness") == "true")
                .proof_test(field("proof_test") == "true")
                .status(status)
                .skip_reason(skip_reason)
                .kani_excepted(field("kani_excepted") == "true")
                .creusot_excepted(field("creusot_excepted") == "true")
                .verus_excepted(field("verus_excepted") == "true")
                .build()?,
        );
    }

    Ok(Some(
        AmenableStdReport::builder()
            .source_crate(source_crate)
            .impl_crate(impl_crate)
            .include_nightly(include_nightly)
            .entries(entries)
            .complete_count(complete_count)
            .partial_count(partial_count)
            .missing_count(missing_count)
            .skipped_count(skipped_count)
            .build()?,
    ))
}

#[instrument(level = "debug", skip(findings))]
pub fn amenable_ext_jiff_gaps_from_findings(findings: &[&dyn Finding]) -> Vec<AmenableStdGapEntry> {
    findings
        .iter()
        .filter(|finding| {
            finding.rule().category() == AMENABLE_EXT_JIFF_CATEGORY
                && finding.disposition() == Disposition::Open
        })
        .map(|finding| {
            let mut sink = crate::objects::MapFindingSink::default();
            finding.emit(&mut sink);
            let field = |name: &str| {
                sink.fields()
                    .iter()
                    .find(|(key, _)| key == name)
                    .map(|(_, value)| value.clone())
                    .unwrap_or_default()
            };
            let status = match field("status").as_str() {
                "Partial" => AmenableStdStatus::Partial,
                "Skipped" => AmenableStdStatus::Skipped,
                "Complete" => AmenableStdStatus::Complete,
                _ => AmenableStdStatus::Missing,
            };
            AmenableStdGapEntry::new(
                field("source_crate"),
                field("type_path"),
                field("type_kind"),
                status,
                field("missing_layers"),
                field("action"),
            )
        })
        .collect()
}

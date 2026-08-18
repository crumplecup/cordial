use crate::framework_std::{
    AmenableStdEntry, AmenableStdGapEntry, AmenableStdReport, AmenableStdStatus,
};
use crate::objects::{Disposition, Finding, FindingSink, IrAnchor, NodeAnchor, Rule};

use super::AMENABLE_STD_CATEGORY;

#[derive(Debug, Clone, Copy)]
pub struct AmenableStdRule;

impl Rule for AmenableStdRule {
    fn id(&self) -> &str {
        "AMENABLE-STD-ROW"
    }

    fn category(&self) -> &str {
        AMENABLE_STD_CATEGORY
    }

    fn description(&self) -> &str {
        "Std inventory row assessed for amenable registry coverage"
    }
}

#[derive(Debug, Clone)]
pub struct AmenableStdRowFinding {
    pub rule: AmenableStdRule,
    pub disposition: Disposition,
    pub anchor: NodeAnchor,
    pub source_crate: String,
    pub impl_crate: String,
    pub type_path: String,
    pub type_kind: String,
    pub is_generic: bool,
    pub status: AmenableStdStatus,
    pub evidence_link: bool,
    pub evidence_name: Option<String>,
    pub kani_witness: bool,
    pub creusot_witness: bool,
    pub verus_witness: bool,
    pub proof_test: bool,
    pub skip_reason: Option<String>,
    pub kani_excepted: bool,
    pub creusot_excepted: bool,
    pub verus_excepted: bool,
    pub missing_layers: String,
    pub action: String,
}

impl Finding for AmenableStdRowFinding {
    fn rule(&self) -> &dyn Rule {
        &self.rule
    }

    fn disposition(&self) -> Disposition {
        self.disposition
    }

    fn anchor(&self) -> &dyn IrAnchor {
        &self.anchor
    }

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

pub fn amenable_row_disposition(status: AmenableStdStatus) -> Disposition {
    match status {
        AmenableStdStatus::Missing | AmenableStdStatus::Partial => Disposition::Open,
        AmenableStdStatus::Skipped => Disposition::Suppressed,
        AmenableStdStatus::Complete => Disposition::Exemplar,
    }
}

pub fn amenable_report_from_findings(
    findings: &[&dyn Finding],
    include_nightly: bool,
) -> Option<AmenableStdReport> {
    let rows: Vec<_> = findings
        .iter()
        .filter(|finding| finding.rule().category() == AMENABLE_STD_CATEGORY)
        .collect();
    if rows.is_empty() {
        return None;
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
            sink.fields
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
        entries.push(AmenableStdEntry {
            type_path: field("type_path"),
            type_kind: field("type_kind"),
            is_generic: field("is_generic") == "true",
            evidence_link: field("evidence_link") == "true",
            evidence_name,
            kani_witness: field("kani_witness") == "true",
            creusot_witness: field("creusot_witness") == "true",
            verus_witness: field("verus_witness") == "true",
            proof_test: field("proof_test") == "true",
            status,
            skip_reason,
            kani_excepted: field("kani_excepted") == "true",
            creusot_excepted: field("creusot_excepted") == "true",
            verus_excepted: field("verus_excepted") == "true",
        });
    }

    Some(AmenableStdReport {
        source_crate,
        impl_crate,
        include_nightly,
        entries,
        complete_count,
        partial_count,
        missing_count,
        skipped_count,
    })
}

pub fn amenable_gaps_from_findings(findings: &[&dyn Finding]) -> Vec<AmenableStdGapEntry> {
    findings
        .iter()
        .filter(|finding| {
            finding.rule().category() == AMENABLE_STD_CATEGORY
                && finding.disposition() == Disposition::Open
        })
        .map(|finding| {
            let mut sink = crate::objects::MapFindingSink::default();
            finding.emit(&mut sink);
            let field = |name: &str| {
                sink.fields
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
            AmenableStdGapEntry {
                source_crate: field("source_crate"),
                type_path: field("type_path"),
                type_kind: field("type_kind"),
                status,
                missing_layers: field("missing_layers"),
                action: field("action"),
            }
        })
        .collect()
}

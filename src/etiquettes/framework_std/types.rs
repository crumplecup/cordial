use crate::framework_std::{
    FrameworkGapEntry, FrameworkTraitEntry, FrameworkTraitReport, FrameworkTraitStatus,
};
use crate::objects::{
    Disposition, Finding, FindingSink, IrAnchor, Marker, NodeAnchor, Rule, SourceSpan,
};
use crate::rustdoc::InventoryItemKind;

pub const HOMECOMING_STD_CATEGORY: &str = "homecoming-std";
pub const AMENABLE_STD_CATEGORY: &str = "amenable-std";

#[derive(Debug, Clone, Copy)]
pub struct FrameworkStdRule;

impl Rule for FrameworkStdRule {
    fn id(&self) -> &str {
        "FRAMEWORK-STD-ROW"
    }

    fn category(&self) -> &str {
        HOMECOMING_STD_CATEGORY
    }

    fn description(&self) -> &str {
        "Std inventory row assessed for framework trait coverage"
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)] // type fields reserved for per-marker assess (R3+)
pub struct FrameworkStdScopeMarker {
    pub anchor: NodeAnchor,
    pub probe_id: String,
    /// Carried for traceability; batch assessor uses full inventory today (R2).
    pub type_path: String,
    pub type_kind: InventoryItemKind,
    pub is_generic: bool,
}

impl Marker for FrameworkStdScopeMarker {
    fn probe(&self) -> &str {
        &self.probe_id
    }

    fn label(&self) -> &str {
        &self.probe_id
    }

    fn anchor(&self) -> &dyn IrAnchor {
        &self.anchor
    }

    fn span(&self) -> Option<&dyn SourceSpan> {
        None
    }

    fn field(&self, key: &str) -> Option<&str> {
        match key {
            "type_path" => Some(&self.type_path),
            "type_kind" => Some(self.type_kind.as_str()),
            "is_generic" => {
                if self.is_generic {
                    Some("true")
                } else {
                    Some("false")
                }
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FrameworkStdRowFinding {
    pub rule: FrameworkStdRule,
    pub disposition: Disposition,
    pub anchor: NodeAnchor,
    pub source_crate: String,
    pub trait_name: String,
    pub impl_crate: String,
    pub type_path: String,
    pub type_kind: String,
    pub is_generic: bool,
    pub trait_status: FrameworkTraitStatus,
    pub skip_reason: Option<String>,
}

impl Finding for FrameworkStdRowFinding {
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
        sink.field("trait_name", &self.trait_name);
        sink.field("impl_crate", &self.impl_crate);
        sink.field("type_path", &self.type_path);
        sink.field("type_kind", &self.type_kind);
        sink.field(
            "is_generic",
            if self.is_generic { &"true" } else { &"false" },
        );
        sink.field("trait_status", &self.trait_status.to_string());
        sink.field("skip_reason", &self.skip_reason.as_deref().unwrap_or(""));
        if self.trait_status == FrameworkTraitStatus::Missing {
            sink.field(
                "action",
                &format!(
                    "Add `impl {} for {}` in {}",
                    self.trait_name, self.type_path, self.impl_crate
                ),
            );
        }
    }
}

pub fn homecoming_row_disposition(status: FrameworkTraitStatus) -> Disposition {
    match status {
        FrameworkTraitStatus::Missing => Disposition::Open,
        FrameworkTraitStatus::Skipped => Disposition::Suppressed,
        FrameworkTraitStatus::Complete => Disposition::Exemplar,
    }
}

pub fn framework_report_from_findings(
    findings: &[&dyn Finding],
    include_nightly: bool,
) -> Option<FrameworkTraitReport> {
    let rows: Vec<_> = findings
        .iter()
        .filter(|finding| finding.rule().category() == HOMECOMING_STD_CATEGORY)
        .collect();
    if rows.is_empty() {
        return None;
    }

    let mut entries = Vec::new();
    let mut complete_count = 0usize;
    let mut missing_count = 0usize;
    let mut skipped_count = 0usize;
    let mut source_crate = String::new();
    let mut trait_name = String::new();
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
            trait_name = field("trait_name");
            impl_crate = field("impl_crate");
        }
        let trait_status = match field("trait_status").as_str() {
            "Complete" => FrameworkTraitStatus::Complete,
            "Skipped" => FrameworkTraitStatus::Skipped,
            _ => FrameworkTraitStatus::Missing,
        };
        match trait_status {
            FrameworkTraitStatus::Complete => complete_count += 1,
            FrameworkTraitStatus::Missing => missing_count += 1,
            FrameworkTraitStatus::Skipped => skipped_count += 1,
        }
        let skip_reason = {
            let reason = field("skip_reason");
            if reason.is_empty() {
                None
            } else {
                Some(reason)
            }
        };
        entries.push(FrameworkTraitEntry {
            type_path: field("type_path"),
            type_kind: field("type_kind"),
            is_generic: field("is_generic") == "true",
            trait_status,
            skip_reason,
        });
    }

    Some(FrameworkTraitReport {
        source_crate,
        trait_name,
        impl_crate,
        include_nightly,
        entries,
        complete_count,
        missing_count,
        skipped_count,
    })
}

pub fn framework_gaps_from_findings(findings: &[&dyn Finding]) -> Vec<FrameworkGapEntry> {
    findings
        .iter()
        .filter(|finding| {
            finding.rule().category() == HOMECOMING_STD_CATEGORY
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
            FrameworkGapEntry {
                source_crate: field("source_crate"),
                type_path: field("type_path"),
                type_kind: field("type_kind"),
                trait_name: field("trait_name"),
                impl_crate: field("impl_crate"),
                action: field("action"),
            }
        })
        .collect()
}

/// Amenable-std-specific rule/finding/report types, gated as a whole unit —
/// see `docs/planning/cfg-scatter-etiquette.md` for the pattern.
#[cfg(feature = "amenable_std")]
mod amenable {
    use super::*;
    use crate::framework_std::{
        AmenableStdEntry, AmenableStdGapEntry, AmenableStdReport, AmenableStdStatus,
    };

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
}

#[cfg(feature = "amenable_std")]
pub use amenable::{
    AmenableStdRowFinding, AmenableStdRule, amenable_gaps_from_findings,
    amenable_report_from_findings, amenable_row_disposition,
};

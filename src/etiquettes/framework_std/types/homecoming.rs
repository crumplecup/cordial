use crate::framework_std::{
    FrameworkGapEntry, FrameworkTraitEntry, FrameworkTraitReport, FrameworkTraitStatus,
};
use crate::objects::{
    Disposition, Finding, FindingSink, IrAnchor, Marker, NodeAnchor, Rule, SourceSpan,
};
use crate::rustdoc::InventoryItemKind;

use super::HOMECOMING_STD_CATEGORY;

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

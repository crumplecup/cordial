//! Finding/Rule for amenable ext (third-party crate) registry coverage —
//! one generic implementation shared by every `amenable-ext-{target}`
//! etiquette instance (jiff today; chrono and others register by calling
//! [`super::build_ext_etiquette`] with their own target name).

use crate::error::CordialResult;
use crate::framework_std::{
    AmenableStdEntry, AmenableStdGapEntry, AmenableStdReport, AmenableStdStatus,
};
use crate::objects::{Disposition, Finding, FindingSink, IrAnchor, NodeAnchor, Rule};

use tracing::instrument;

/// Etiquette id / finding category for `target` (`"jiff"` ->
/// `"amenable-ext-jiff"`). Shared by the etiquette, the rule, and every
/// report/gap filter so they agree on one string.
#[instrument(level = "trace")]
pub fn ext_etiquette_id(target: &str) -> String {
    format!("amenable-ext-{target}")
}

#[instrument(level = "trace")]
fn capitalize(target: &str) -> String {
    let mut chars = target.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Rule for one amenable-ext target's inventory rows. Holds the target's
/// derived id/category/description as owned strings so a target name read
/// from `cordial.toml` at runtime can mint one without a `'static` literal.
#[derive(Debug, Clone)]
pub struct ExtRowRule {
    id: String,
    category: String,
    description: String,
}

impl ExtRowRule {
    /// Build the row rule for `target` (e.g. `"jiff"`), deriving
    /// id/category/description the same way every amenable-ext target does.
    #[instrument(level = "debug")]
    pub fn new(target: &str) -> Self {
        Self {
            id: format!("AMENABLE-EXT-{}-ROW", target.to_uppercase()),
            category: ext_etiquette_id(target),
            description: format!(
                "{} inventory row assessed for amenable_ext registry coverage",
                capitalize(target)
            ),
        }
    }
}

impl Rule for ExtRowRule {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        &self.id
    }

    #[instrument(level = "trace", skip(self))]
    fn category(&self) -> &str {
        &self.category
    }

    #[instrument(level = "trace", skip(self))]
    fn description(&self) -> &str {
        &self.description
    }
}

/// One row of an amenable-ext target's coverage: a plain inventory row, a
/// generic row that aggregates its instantiations, or one instantiation.
#[derive(Debug, Clone, derive_builder::Builder)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub struct ExtRowFinding {
    rule: ExtRowRule,
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
    /// For an instantiation row: the generic row it belongs to.
    #[builder(default)]
    parent: Option<String>,
    /// A parent's roll-up and declared bounds, or why a child is unusual.
    #[builder(default)]
    note: Option<String>,
    /// How many instantiation rows follow this generic row; 0 for any other.
    #[builder(default)]
    instantiations: usize,
}

impl ExtRowFinding {
    /// Start a builder for this finding.
    #[instrument(level = "debug")]
    pub fn builder() -> ExtRowFindingBuilder {
        ExtRowFindingBuilder::default()
    }
}

impl Finding for ExtRowFinding {
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
        sink.field("parent", &self.parent.as_deref().unwrap_or(""));
        sink.field("note", &self.note.as_deref().unwrap_or(""));
        sink.field("instantiations", &self.instantiations.to_string());
        sink.field(
            "kind",
            &if self.parent.is_some() {
                "instantiation"
            } else if self.instantiations > 0 {
                "aggregate"
            } else {
                "plain"
            },
        );
    }
}

/// How a row's status maps to a finding disposition: Missing and Partial are
/// open, Skipped is suppressed, Complete is an exemplar.
#[instrument(level = "debug", skip(status))]
pub fn ext_row_disposition(status: AmenableStdStatus) -> Disposition {
    match status {
        AmenableStdStatus::Missing | AmenableStdStatus::Partial => Disposition::Open,
        AmenableStdStatus::Skipped => Disposition::Suppressed,
        AmenableStdStatus::Complete => Disposition::Exemplar,
    }
}

/// `None` for an empty field, else the text.
#[instrument(level = "trace")]
fn non_empty(text: String) -> Option<String> {
    (!text.is_empty()).then_some(text)
}

/// Rebuild the coverage report for `category` from the findings the assessor
/// emitted, with instantiation rows and notes intact. `None` when there are
/// no findings in that category.
#[instrument(level = "debug", skip(findings))]
pub fn ext_report_from_findings(
    findings: &[&dyn Finding],
    category: &str,
    include_nightly: bool,
) -> CordialResult<Option<AmenableStdReport>> {
    let rows: Vec<_> = findings
        .iter()
        .filter(|finding| finding.rule().category() == category)
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
                .parent(non_empty(field("parent")))
                .note(non_empty(field("note")))
                .instantiations(field("instantiations").parse().unwrap_or(0))
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

/// The open rows of `category` as gap entries. An aggregate generic row is
/// left out: it is open only because its instantiation rows are.
#[instrument(level = "debug", skip(findings))]
pub fn ext_gaps_from_findings(
    findings: &[&dyn Finding],
    category: &str,
) -> Vec<AmenableStdGapEntry> {
    findings
        .iter()
        .filter(|finding| {
            finding.rule().category() == category && finding.disposition() == Disposition::Open
        })
        .filter_map(|finding| {
            let mut sink = crate::objects::MapFindingSink::default();
            finding.emit(&mut sink);
            let field = |name: &str| {
                sink.fields()
                    .iter()
                    .find(|(key, _)| key == name)
                    .map(|(_, value)| value.clone())
                    .unwrap_or_default()
            };
            // An aggregate is open only because its instantiation rows are;
            // those rows are the gaps.
            if field("kind") == "aggregate" {
                return None;
            }
            let status = match field("status").as_str() {
                "Partial" => AmenableStdStatus::Partial,
                "Skipped" => AmenableStdStatus::Skipped,
                "Complete" => AmenableStdStatus::Complete,
                _ => AmenableStdStatus::Missing,
            };
            Some(AmenableStdGapEntry::new(
                field("source_crate"),
                field("type_path"),
                field("type_kind"),
                status,
                field("missing_layers"),
                field("action"),
            ))
        })
        .collect()
}

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::etiquettes::error_chain::{ErrorChainProbeId, ErrorChainRecord};
use crate::etiquettes::error_sites::ErrorSiteKind;
use crate::etiquettes::foreign_error_types::ForeignErrorTypeRecord;

use super::types::{
    ErrorHandlingResolutionId, ForeignErrorAttenuationRecord, ForeignErrorAttenuationReport,
    ForeignErrorHandlingClass,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SiteKey {
    file: PathBuf,
    line: u32,
}

/// Merge typed foreign sites with positive/negative chain probe results.
pub fn build_foreign_error_attenuation_report(
    foreign_report: &crate::etiquettes::foreign_error_types::ForeignErrorTypeReport,
    chain_records: &[ErrorChainRecord],
) -> ForeignErrorAttenuationReport {
    build_foreign_error_attenuation_report_with_bridges(foreign_report, chain_records, &[])
}

/// Typed crate-error constructor that already keeps a foreign error as `source`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ErrorBridgeHint {
    pub foreign_type: String,
    pub constructor: String,
}

pub(crate) fn build_foreign_error_attenuation_report_with_bridges(
    foreign_report: &crate::etiquettes::foreign_error_types::ForeignErrorTypeReport,
    chain_records: &[ErrorChainRecord],
    bridges: &[ErrorBridgeHint],
) -> ForeignErrorAttenuationReport {
    let preserved_by_site = index_preserved_propagation_sites(chain_records);
    let map_err_sites: HashSet<SiteKey> = foreign_report
        .findings
        .iter()
        .filter(|foreign| foreign.kind == ErrorSiteKind::MapErr)
        .map(|foreign| SiteKey {
            file: foreign.file.clone(),
            line: foreign.line,
        })
        .collect();
    let findings = foreign_report
        .findings
        .iter()
        .filter(|foreign| {
            foreign.kind != ErrorSiteKind::QuestionMark
                || !map_err_sites.contains(&SiteKey {
                    file: foreign.file.clone(),
                    line: foreign.line,
                })
        })
        .map(|foreign| classify_foreign_site(foreign, &preserved_by_site, bridges))
        .collect();

    ForeignErrorAttenuationReport {
        crate_name: foreign_report.crate_name.clone(),
        findings,
    }
}

fn index_preserved_propagation_sites(
    chain_records: &[ErrorChainRecord],
) -> HashMap<SiteKey, ErrorChainRecord> {
    let mut map = HashMap::new();
    for record in chain_records {
        if !is_propagation_probe(record.rule_id) {
            continue;
        }
        map.insert(
            SiteKey {
                file: record.file.clone(),
                line: record.line,
            },
            record.clone(),
        );
    }
    map
}

fn is_propagation_probe(rule_id: ErrorChainProbeId) -> bool {
    matches!(
        rule_id,
        ErrorChainProbeId::PreservedQuestionMark001 | ErrorChainProbeId::PreservedMapErr001
    )
}

fn classify_foreign_site(
    foreign: &ForeignErrorTypeRecord,
    preserved_by_site: &HashMap<SiteKey, ErrorChainRecord>,
    bridges: &[ErrorBridgeHint],
) -> ForeignErrorAttenuationRecord {
    let key = SiteKey {
        file: foreign.file.clone(),
        line: foreign.line,
    };
    if let Some(preserved) = preserved_by_site.get(&key) {
        return ForeignErrorAttenuationRecord {
            crate_name: foreign.crate_name.clone(),
            foreign_error_type: foreign.foreign_error_type.clone(),
            inference_rule_id: foreign.rule_id.clone(),
            confidence: foreign.confidence,
            handling_class: ForeignErrorHandlingClass::ChainPreserved,
            resolution_id: ErrorHandlingResolutionId::MaintainExemplar,
            resolution: "Reference pattern — keep the foreign error as `source` on a crate \
                          error newtype (or `From` bridge) and propagate with `?` or `map_err`."
                .to_string(),
            kind: foreign.kind,
            context: foreign.context.clone(),
            file: foreign.file.clone(),
            line: foreign.line,
            source_snippet: foreign.source_snippet.clone(),
            site_snippet: foreign.site_snippet.clone(),
            good_pattern: preserved.snippet.clone(),
            bad_pattern: String::new(),
        };
    }

    if foreign.chain_break {
        let (resolution_id, resolution, good_pattern) = chain_break_resolution(
            &foreign.foreign_error_type,
            &foreign.source_snippet,
            bridges,
        );
        return ForeignErrorAttenuationRecord {
            crate_name: foreign.crate_name.clone(),
            foreign_error_type: foreign.foreign_error_type.clone(),
            inference_rule_id: foreign.rule_id.clone(),
            confidence: foreign.confidence,
            handling_class: ForeignErrorHandlingClass::ChainBreak,
            resolution_id,
            resolution,
            kind: foreign.kind,
            context: foreign.context.clone(),
            file: foreign.file.clone(),
            line: foreign.line,
            source_snippet: foreign.source_snippet.clone(),
            site_snippet: foreign.site_snippet.clone(),
            good_pattern,
            bad_pattern: foreign.site_snippet.clone(),
        };
    }

    if foreign.kind == ErrorSiteKind::QuestionMark {
        if foreign.source_snippet.contains(".ok(") || foreign.site_snippet.contains(".ok(") {
            return option_ok_neutral_record(foreign);
        }
        if find_bridge(bridges, &foreign.foreign_error_type).is_some() {
            return ForeignErrorAttenuationRecord {
                crate_name: foreign.crate_name.clone(),
                foreign_error_type: foreign.foreign_error_type.clone(),
                inference_rule_id: foreign.rule_id.clone(),
                confidence: foreign.confidence,
                handling_class: ForeignErrorHandlingClass::ChainPreserved,
                resolution_id: ErrorHandlingResolutionId::MaintainExemplar,
                resolution: "Reference pattern — `From` already keeps the foreign error, so `?` \
                              preserves the chain."
                    .to_string(),
                kind: foreign.kind,
                context: foreign.context.clone(),
                file: foreign.file.clone(),
                line: foreign.line,
                source_snippet: foreign.source_snippet.clone(),
                site_snippet: foreign.site_snippet.clone(),
                good_pattern: format!("{}?", foreign.source_snippet),
                bad_pattern: String::new(),
            };
        }
        let (resolution_id, resolution, good_pattern) = pending_infrastructure_resolution(
            &foreign.foreign_error_type,
            &foreign.source_snippet,
            bridges,
        );
        return ForeignErrorAttenuationRecord {
            crate_name: foreign.crate_name.clone(),
            foreign_error_type: foreign.foreign_error_type.clone(),
            inference_rule_id: foreign.rule_id.clone(),
            confidence: foreign.confidence,
            handling_class: ForeignErrorHandlingClass::PendingInfrastructure,
            resolution_id,
            resolution,
            kind: foreign.kind,
            context: foreign.context.clone(),
            file: foreign.file.clone(),
            line: foreign.line,
            source_snippet: foreign.source_snippet.clone(),
            site_snippet: foreign.site_snippet.clone(),
            good_pattern,
            bad_pattern: foreign.site_snippet.clone(),
        };
    }

    ForeignErrorAttenuationRecord {
        crate_name: foreign.crate_name.clone(),
        foreign_error_type: foreign.foreign_error_type.clone(),
        inference_rule_id: foreign.rule_id.clone(),
        confidence: foreign.confidence,
        handling_class: ForeignErrorHandlingClass::Neutral,
        resolution_id: ErrorHandlingResolutionId::ManualReview,
        resolution: "Review typed foreign exposure — not a chain-break `map_err` or preserved \
                      propagation site."
            .to_string(),
        kind: foreign.kind,
        context: foreign.context.clone(),
        file: foreign.file.clone(),
        line: foreign.line,
        source_snippet: foreign.source_snippet.clone(),
        site_snippet: foreign.site_snippet.clone(),
        good_pattern: good_pattern_template(
            &foreign.foreign_error_type,
            &foreign.source_snippet,
            bridges,
        ),
        bad_pattern: foreign.site_snippet.clone(),
    }
}

fn option_ok_neutral_record(foreign: &ForeignErrorTypeRecord) -> ForeignErrorAttenuationRecord {
    ForeignErrorAttenuationRecord {
        crate_name: foreign.crate_name.clone(),
        foreign_error_type: foreign.foreign_error_type.clone(),
        inference_rule_id: foreign.rule_id.clone(),
        confidence: foreign.confidence,
        handling_class: ForeignErrorHandlingClass::Neutral,
        resolution_id: ErrorHandlingResolutionId::ManualReview,
        resolution: "Option propagation (`.ok()?`) — not a foreign `Result` chain boundary."
            .to_string(),
        kind: foreign.kind,
        context: foreign.context.clone(),
        file: foreign.file.clone(),
        line: foreign.line,
        source_snippet: foreign.source_snippet.clone(),
        site_snippet: foreign.site_snippet.clone(),
        good_pattern: String::new(),
        bad_pattern: foreign.site_snippet.clone(),
    }
}

fn chain_break_resolution(
    foreign_error_type: &str,
    source_snippet: &str,
    bridges: &[ErrorBridgeHint],
) -> (ErrorHandlingResolutionId, String, String) {
    let good = good_pattern_template(foreign_error_type, source_snippet, bridges);
    let resolution = match find_bridge(bridges, foreign_error_type) {
        Some(bridge) => format!(
            "`{constructor}` already keeps `{foreign_error_type}` as source. Replace the \
             stringifying `.map_err` with `{good}`. Do not stringify into `String` or \
             `invariant(format!(…{{err}}))`.",
            constructor = bridge.constructor,
        ),
        None => format!(
            "Introduce a crate error newtype (or enum variant) that holds `{foreign_error_type}` \
             as `source` and implements `std::error::Error` + `From<{foreign_error_type}>`. Then \
             `{good}`. Do not use `Result<_, String>` or stringify with `.to_string()` / \
             `format!(\"...{{err}}\")`."
        ),
    };
    (
        ErrorHandlingResolutionId::ReplaceStringifyingMapErr,
        resolution,
        good,
    )
}

fn pending_infrastructure_resolution(
    foreign_error_type: &str,
    source_snippet: &str,
    bridges: &[ErrorBridgeHint],
) -> (ErrorHandlingResolutionId, String, String) {
    let good = format!("{source_snippet}?");
    let resolution = match find_bridge(bridges, foreign_error_type) {
        Some(bridge) => format!(
            "`{constructor}` already keeps `{foreign_error_type}` as source, so `{good}` \
             preserves the chain.",
            constructor = bridge.constructor,
        ),
        None => format!(
            "Add a crate error newtype that holds `{foreign_error_type}` as `source` and \
             `impl From<{foreign_error_type}>` so `{good}` preserves the foreign error."
        ),
    };
    (
        ErrorHandlingResolutionId::AddInfrastructureThenQuestionMark,
        resolution,
        good,
    )
}

fn good_pattern_template(
    foreign_error_type: &str,
    source_snippet: &str,
    bridges: &[ErrorBridgeHint],
) -> String {
    match find_bridge(bridges, foreign_error_type) {
        Some(bridge) => {
            let enum_name = bridge
                .constructor
                .rsplit_once("::")
                .map(|(prefix, _)| prefix)
                .unwrap_or(bridge.constructor.as_str());
            format!("{source_snippet}.map_err({enum_name}::from)?")
        }
        None => format!("{source_snippet}.map_err(CrateError::from)?"),
    }
}

fn find_bridge<'a>(
    bridges: &'a [ErrorBridgeHint],
    foreign_error_type: &str,
) -> Option<&'a ErrorBridgeHint> {
    bridges
        .iter()
        .find(|bridge| bridge.foreign_type == foreign_error_type)
}

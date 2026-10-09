//! Turn a parsed [`HackRun`] into findings, with the advice for each.

use std::collections::BTreeSet;
use std::path::Path;

use super::{HackRun, SiteHits};
use crate::error::CordialResult;
use crate::etiquettes::feature_warnings::gate::{FeatureSet, Gate, suggest_gate};
use crate::etiquettes::feature_warnings::types::{FeatureWarningRecord, FeatureWarningRuleId};

use tracing::instrument;

/// Turn a parsed run into one record per feature-dependent site.
#[instrument(level = "debug", skip(run, ignore_features))]
pub fn records_from_run(
    run: &HackRun,
    resolve_root: &Path,
    include_universal: bool,
    ignore_features: &BTreeSet<String>,
    private_feature_threshold: usize,
) -> CordialResult<Vec<FeatureWarningRecord>> {
    let total = run.combos.len();
    let mut records = Vec::new();
    for ((file, line, lint), hits) in &run.sites {
        let silent: Vec<FeatureSet> = run.combos.difference(&hits.combos).cloned().collect();
        if silent.is_empty() && !include_universal {
            continue;
        }
        let triggering: Vec<FeatureSet> = hits.combos.iter().cloned().collect();
        let gate = suggest_gate(
            &without_features(&triggering, ignore_features),
            &compiled_silent(&triggering, &silent, ignore_features),
        );
        records.push(
            FeatureWarningRecord::builder()
                .rule_id(FeatureWarningRuleId::for_lint(lint))
                .lint(lint.clone())
                .file(resolve_root.join(file))
                .line(*line)
                .message(site_message(lint, hits))
                .gate(gate.cfg_expression().unwrap_or_default())
                .wide(gate.feature_count() > private_feature_threshold)
                .advice(advice_for(lint, &gate, private_feature_threshold))
                .triggering(describe_triggering(&triggering, total))
                .build()?,
        );
    }
    let attempted = total + run.failed_combos.len();
    for ((file, line, code), hits) in &run.failures {
        let example = hits
            .combos
            .iter()
            .min_by_key(|combo| combo.len())
            .map(|combo| format!(", e.g. `{combo}`"))
            .unwrap_or_default();
        records.push(
            FeatureWarningRecord::builder()
                .rule_id(FeatureWarningRuleId::Failure003)
                .lint(code.clone())
                .file(resolve_root.join(file))
                .line(*line)
                .message(hits.message.clone())
                .gate(String::new())
                .wide(false)
                .advice(failure_advice(code))
                .triggering(format!(
                    "fails in {} of {attempted} combinations{example}",
                    hits.combos.len()
                ))
                .build()?,
        );
    }
    if let Some(message) = &run.hack_failure {
        records.push(
            FeatureWarningRecord::builder()
                .rule_id(FeatureWarningRuleId::Failure003)
                .lint("cargo-hack".to_string())
                .file(resolve_root.join("Cargo.toml"))
                .line(1)
                .message(message.clone())
                .gate(String::new())
                .wide(false)
                .advice(
                    "`cargo hack` failed before or outside any build, so no feature \
                     combination could be checked. Run `cargo hack check \
                     --feature-powerset` by hand to see the full error; a manifest or \
                     dependency problem is the usual cause."
                        .to_string(),
                )
                .triggering("the whole run".to_string())
                .build()?,
        );
    }
    records.sort_by(|left, right| {
        left.file()
            .cmp(right.file())
            .then(left.line().cmp(&right.line()))
            .then(left.lint().cmp(right.lint()))
    });
    Ok(records)
}

/// What to do about a combination that does not compile.
#[instrument(level = "debug")]
pub fn failure_advice(code: &str) -> String {
    let cause = match code {
        "E0432" | "E0433" | "E0412" | "E0425" | "E0405" | "E0599" | "E0603" => {
            "A name used here is gated out in these combinations: the definition sits \
             behind a feature that these combinations do not enable."
        }
        _ => "The compiler rejects this code in these combinations.",
    };
    format!(
        "{cause} Fix it before anything else: warnings in a combination that does \
         not compile cannot be assessed. Either make the feature that gates the \
         definition imply the one that gates this use (in `Cargo.toml`), or give this \
         use the same `cfg` as the definition."
    )
}

/// `combos` with `ignored` features removed. Umbrella features (`default`,
/// `full`, ...) are not something a `cfg` gate should name.
#[instrument(level = "debug", skip(combos, ignored))]
fn without_features(combos: &[FeatureSet], ignored: &BTreeSet<String>) -> Vec<FeatureSet> {
    combos
        .iter()
        .map(|combo| combo.difference(ignored).cloned().collect())
        .collect()
}

/// The silent combinations in which the item is compiled at all.
///
/// Silence has two causes: the item is used, or the item's own gate is off so
/// it does not exist. Only the first says anything about the consumer. The
/// item exists wherever it can warn, so a silent combination counts when it
/// contains every feature common to the triggering ones.
#[instrument(level = "debug", skip(triggering, silent, ignored))]
fn compiled_silent(
    triggering: &[FeatureSet],
    silent: &[FeatureSet],
    ignored: &BTreeSet<String>,
) -> Vec<FeatureSet> {
    let triggering = without_features(triggering, ignored);
    let silent = without_features(silent, ignored);
    let Some(first) = triggering.first() else {
        return silent;
    };
    let common = triggering.iter().skip(1).fold(first.clone(), |acc, combo| {
        acc.intersection(combo).cloned().collect()
    });
    silent
        .into_iter()
        .filter(|combo| common.is_subset(combo))
        .collect()
}

/// The message shown for a site. Unused-import messages list different names
/// per combination, so merge them; every other lint keeps its own text.
#[instrument(level = "trace", skip(hits))]
fn site_message(lint: &str, hits: &SiteHits) -> String {
    if lint != "unused_imports" || hits.names.len() < 2 {
        return hits.message.clone();
    }
    let names: Vec<String> = hits.names.iter().map(|name| format!("`{name}`")).collect();
    format!("unused imports: {}", names.join(", "))
}

#[instrument(level = "debug", skip(combo))]
fn describe_combo(combo: &FeatureSet) -> String {
    if combo.is_empty() {
        return "--no-default-features".to_string();
    }
    let features: Vec<&str> = combo.iter().map(String::as_str).collect();
    format!("--no-default-features --features {}", features.join(","))
}

#[instrument(level = "trace", skip(triggering))]
fn describe_triggering(triggering: &[FeatureSet], total: usize) -> String {
    let example = triggering
        .iter()
        .min_by_key(|combo| (combo.len(), (*combo).clone()));
    match example {
        Some(combo) => format!(
            "{} of {total} combinations, e.g. `{}`",
            triggering.len(),
            describe_combo(combo)
        ),
        None => format!("0 of {total} combinations"),
    }
}

/// What to do about a warning, given the lint and the suggested gate.
#[instrument(level = "debug", skip(gate))]
pub fn advice_for(lint: &str, gate: &Gate, private_feature_threshold: usize) -> String {
    if gate.feature_count() > private_feature_threshold {
        return format!(
            "{} features reach this item, too many for a readable `cfg`. Add a \
             private feature (name it with a leading `_`, e.g. `_<area>_support`) \
             that each of them enables, and gate on that; or move the item next \
             to its only consumer if it has one.",
            gate.feature_count()
        );
    }
    let Some(cfg) = gate.cfg_expression() else {
        return "No single feature gate separates the combinations. If several \
                features share this item, give them a common internal feature; \
                if one consumer uses it, move the item next to that consumer."
            .to_string();
    };
    let what = match lint {
        "unused_imports" => {
            "Gate the `use` (or move it into the gated module that uses it)".to_string()
        }
        "dead_code" => {
            "Gate the definition, or move it into the file of its only consumer".to_string()
        }
        "unused_variables" | "unused_mut" => {
            "Gate the statement, or the `mut`, that only those features need".to_string()
        }
        _ => "Gate the item".to_string(),
    };
    format!("{what} with `#[cfg({cfg})]`.")
}

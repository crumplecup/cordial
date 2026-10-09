//! Expand generic rows into a parent and its instantiation rows.

use tracing::instrument;

use crate::error::CordialResult;
use crate::framework_std::amenable::{RowFacts, entry_from_facts};
use crate::framework_std::type_identity::{TypeKey, TypeResolver, Unresolved};
use crate::framework_std::verifier_skip::VerifierSkipMap;
use crate::framework_std::{AmenableStdEntry, AmenableStdReport, AmenableStdStatus};

use super::evidence::InstantiationEvidence;
use super::expected::ExpectedInstantiations;
use super::note::{ChildView, declared_bounds_note, rollup_summary, short_label};

/// What expanding a report needs besides the report.
#[derive(derive_new::new)]
pub struct InstantiationContext<'a> {
    resolver: &'a dyn TypeResolver,
    evidence: &'a dyn InstantiationEvidence,
    skip_map: &'a VerifierSkipMap,
    expected: &'a ExpectedInstantiations,
}

/// One instantiation to report: its label, and its key or why there is none.
struct Seed {
    label: String,
    key: Result<TypeKey, Unresolved>,
    expected: bool,
}

/// Replace each generic row that has instantiations with its aggregate
/// parent followed by one child row per instantiation, recounting the
/// report. Rows without instantiations pass through unchanged.
#[instrument(level = "debug", skip(report, ctx), err(level = "warn"))]
pub fn expand_report(
    report: &AmenableStdReport,
    ctx: &InstantiationContext<'_>,
) -> CordialResult<AmenableStdReport> {
    let mut entries = Vec::new();
    for entry in report.entries() {
        entries.extend(expand_entry(entry, ctx)?);
    }
    let count = |status: AmenableStdStatus| {
        entries
            .iter()
            .filter(|entry| entry.status() == status)
            .count()
    };
    AmenableStdReport::builder()
        .source_crate(report.source_crate().clone())
        .impl_crate(report.impl_crate().clone())
        .include_nightly(report.include_nightly())
        .complete_count(count(AmenableStdStatus::Complete))
        .partial_count(count(AmenableStdStatus::Partial))
        .missing_count(count(AmenableStdStatus::Missing))
        .skipped_count(count(AmenableStdStatus::Skipped))
        .entries(entries)
        .build()
}

/// `[parent, children..]` for a generic row, or just `[entry]`.
#[instrument(level = "debug", skip(parent, ctx), err(level = "warn"))]
fn expand_entry(
    parent: &AmenableStdEntry,
    ctx: &InstantiationContext<'_>,
) -> CordialResult<Vec<AmenableStdEntry>> {
    if !parent.is_generic() || parent.status() == AmenableStdStatus::Skipped {
        return Ok(vec![parent.clone()]);
    }
    let Ok(head) = ctx.resolver.resolve_head(parent.type_path()) else {
        return Ok(vec![parent.clone()]);
    };
    let seeds = seeds_for(parent, head.head(), ctx);
    if seeds.is_empty() {
        return Ok(vec![parent.clone()]);
    }

    let mut children = Vec::with_capacity(seeds.len());
    for seed in &seeds {
        children.push(child_entry(parent, seed, ctx)?);
    }
    let views: Vec<ChildView<'_>> = seeds
        .iter()
        .map(|seed| {
            ChildView::new(
                seed.key
                    .as_ref()
                    .map(short_label)
                    .unwrap_or_else(|_| seed.label.clone()),
                seed.key.as_ref().ok(),
            )
        })
        .collect();
    let mut note = rollup_summary(&children);
    note.push_str(". ");
    note.push_str(&declared_bounds_note(&head, &views, ctx.resolver));
    let mut rows = vec![parent.clone().rolled_up(&children, Some(note))];
    rows.extend(children);
    Ok(rows)
}

/// The expected instantiations first (in configured order), then any
/// registered instantiation that was not expected.
#[instrument(level = "debug", skip(parent, ctx))]
fn seeds_for(parent: &AmenableStdEntry, head: &str, ctx: &InstantiationContext<'_>) -> Vec<Seed> {
    let mut seeds: Vec<Seed> = ctx
        .expected
        .tuples_for(parent.type_path())
        .iter()
        .map(|tuple| {
            let label = format!("{}<{}>", parent.type_path(), tuple.join(", "));
            Seed {
                key: ctx.resolver.resolve(&label),
                label,
                expected: true,
            }
        })
        .collect();
    let mut extras: Vec<TypeKey> = ctx
        .evidence
        .instantiations_of(head)
        .into_iter()
        .filter(|found| {
            !seeds
                .iter()
                .any(|seed| seed.key.as_ref().is_ok_and(|key| key == found))
        })
        .collect();
    extras.sort_by_key(|key| key.to_string());
    extras.dedup();
    seeds.extend(extras.into_iter().map(|key| Seed {
        label: key.to_string(),
        key: Ok(key),
        expected: false,
    }));
    seeds
}

#[instrument(level = "debug", skip(parent, seed, ctx), err(level = "warn"))]
fn child_entry(
    parent: &AmenableStdEntry,
    seed: &Seed,
    ctx: &InstantiationContext<'_>,
) -> CordialResult<AmenableStdEntry> {
    let origin_note =
        (!seed.expected).then(|| "registered but not in the expected list".to_string());
    match &seed.key {
        Ok(key) => {
            let verifiers = ctx.evidence.verifiers_for(key);
            let exception = ctx
                .skip_map
                .get(&seed.label)
                .or_else(|| ctx.skip_map.get(&key.to_string()));
            let entry = entry_from_facts(RowFacts::new(
                &seed.label,
                parent.type_kind(),
                false,
                ctx.evidence.evidence_name_for(key),
                &verifiers,
                ctx.evidence.has_proof_test(key),
                exception,
            ))?;
            Ok(entry.into_instantiation(parent.type_path(), origin_note))
        }
        Err(reason) => {
            let none = std::collections::HashSet::new();
            let entry = entry_from_facts(RowFacts::new(
                &seed.label,
                parent.type_kind(),
                false,
                None,
                &none,
                false,
                ctx.skip_map.get(&seed.label),
            ))?;
            Ok(entry.into_instantiation(
                parent.type_path(),
                Some(format!("type could not be identified: {reason}")),
            ))
        }
    }
}

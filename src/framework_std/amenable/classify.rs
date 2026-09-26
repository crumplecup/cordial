use std::collections::HashSet;

use tracing::instrument;

use crate::error::CordialResult;
use crate::framework_std::StdInventoryItem;
use crate::framework_std::registry::{
    RegistryDump, evidence_for_ext_type, evidence_for_std_type, ext_type_has_proof_test,
    std_type_has_proof_test, witness_verifiers_for_ext_type, witness_verifiers_for_std_type,
};
use crate::framework_std::types::framework_std_type_items;
use crate::framework_std::verifier_skip::VerifierSkipMap;

use super::{AmenableStdEntry, AmenableStdReport, AmenableStdStatus};

/// Build a registry coverage report, classifying each row with
/// `classify_row` -- shared by [`build_amenable_std_report`] and
/// [`build_amenable_ext_report`], which differ only in which wrapper
/// type's evidence/witness/proof-test lookups `classify_row` uses.
#[instrument(level = "debug", skip(args, classify_row))]
fn build_wrapped_report(
    args: BuildWrappedReportArgs<'_>,
    classify_row: impl Fn(&str, ClassifyRowArgs<'_>) -> CordialResult<AmenableStdEntry>,
) -> CordialResult<AmenableStdReport> {
    let mut entries = Vec::new();
    let mut complete_count = 0usize;
    let mut partial_count = 0usize;
    let mut missing_count = 0usize;
    let mut skipped_count = 0usize;

    for item in framework_std_type_items(args.items, args.include_nightly) {
        let entry = classify_row(
            item.path(),
            ClassifyRowArgs::new(
                item.kind().as_str(),
                item.is_generic(),
                item.alias_target().as_deref(),
                args.items,
                args.registry,
                args.skip_map,
                args.proof_chain_subjects,
            ),
        )?;
        match entry.status() {
            AmenableStdStatus::Complete => complete_count += 1,
            AmenableStdStatus::Partial => partial_count += 1,
            AmenableStdStatus::Missing => missing_count += 1,
            AmenableStdStatus::Skipped => skipped_count += 1,
        }
        entries.push(entry);
    }

    AmenableStdReport::builder()
        .source_crate(args.source_crate.to_string())
        .impl_crate(args.impl_crate.to_string())
        .include_nightly(args.include_nightly)
        .entries(entries)
        .complete_count(complete_count)
        .partial_count(partial_count)
        .missing_count(missing_count)
        .skipped_count(skipped_count)
        .build()
}

/// Build an amenable std registry coverage report.
#[instrument(level = "debug", skip(items, registry, skip_map, proof_chain_subjects))]
pub fn build_amenable_std_report(
    source_crate: &str,
    items: &[StdInventoryItem],
    impl_crate: &str,
    registry: &RegistryDump,
    skip_map: &VerifierSkipMap,
    proof_chain_subjects: &HashSet<String>,
    include_nightly: bool,
) -> CordialResult<AmenableStdReport> {
    build_wrapped_report(
        BuildWrappedReportArgs::new(
            source_crate,
            items,
            impl_crate,
            registry,
            skip_map,
            proof_chain_subjects,
            include_nightly,
        ),
        classify_amenable_std_row,
    )
}

/// Build an amenable ext (third-party crate) registry coverage report.
#[instrument(level = "debug", skip(items, registry, skip_map, proof_chain_subjects))]
pub fn build_amenable_ext_report(
    source_crate: &str,
    items: &[StdInventoryItem],
    impl_crate: &str,
    registry: &RegistryDump,
    skip_map: &VerifierSkipMap,
    proof_chain_subjects: &HashSet<String>,
    include_nightly: bool,
) -> CordialResult<AmenableStdReport> {
    build_wrapped_report(
        BuildWrappedReportArgs::new(
            source_crate,
            items,
            impl_crate,
            registry,
            skip_map,
            proof_chain_subjects,
            include_nightly,
        ),
        classify_amenable_ext_row,
    )
}

/// Everything [`classify_amenable_std_row`] needs beyond the row's own
/// `type_path`, bundled so the function takes two arguments instead of
/// eight.
#[derive(derive_new::new)]
struct BuildWrappedReportArgs<'a> {
    source_crate: &'a str,
    items: &'a [StdInventoryItem],
    impl_crate: &'a str,
    registry: &'a RegistryDump,
    skip_map: &'a VerifierSkipMap,
    proof_chain_subjects: &'a HashSet<String>,
    include_nightly: bool,
}

#[derive(derive_new::new)]
pub struct ClassifyRowArgs<'a> {
    type_kind: &'a str,
    is_generic: bool,
    alias_target: Option<&'a str>,
    items: &'a [StdInventoryItem],
    registry: &'a RegistryDump,
    skip_map: &'a VerifierSkipMap,
    proof_chain_subjects: &'a HashSet<String>,
}

/// Classify one inventory row for amenable registry coverage, using
/// `evidence_for_type`/`witness_verifiers_for_type`/`type_has_proof_test`
/// to look up its wrapper-type registration -- shared by
/// [`classify_amenable_std_row`] and [`classify_amenable_ext_row`],
/// which differ only in which wrapper type (`RustStdStandard<T>` or
/// `ExtStandard<T>`) those lookups target.
#[instrument(
    level = "debug",
    skip(
        args,
        evidence_for_type,
        witness_verifiers_for_type,
        type_has_proof_test
    )
)]
fn classify_wrapped_row(
    type_path: &str,
    args: ClassifyRowArgs<'_>,
    evidence_for_type: impl Fn(&RegistryDump, &str) -> Option<String>,
    witness_verifiers_for_type: impl Fn(&RegistryDump, &str) -> HashSet<String>,
    type_has_proof_test: impl Fn(&HashSet<String>, &str) -> bool,
) -> CordialResult<AmenableStdEntry> {
    let ClassifyRowArgs {
        type_kind,
        is_generic,
        alias_target,
        items,
        registry,
        skip_map,
        proof_chain_subjects,
    } = args;
    let exception = skip_map.get(type_path);

    if let Some(exception) = exception
        && exception.verifiers().is_none()
    {
        return AmenableStdEntry::builder()
            .type_path(type_path.to_string())
            .type_kind(type_kind.to_string())
            .is_generic(is_generic)
            .evidence_link(false)
            .evidence_name(None)
            .kani_witness(false)
            .creusot_witness(false)
            .verus_witness(false)
            .proof_test(false)
            .status(AmenableStdStatus::Skipped)
            .skip_reason(Some(exception.reason().clone()))
            .kani_excepted(true)
            .creusot_excepted(true)
            .verus_excepted(true)
            .build();
    }

    let mut evidence_name = evidence_for_type(registry, type_path);
    let mut verifiers = witness_verifiers_for_type(registry, type_path);
    let mut proof_test = type_has_proof_test(proof_chain_subjects, type_path);
    if evidence_name.is_none()
        && let Some(target) = alias_target
    {
        let resolved_target = resolve_alias_chain(items, target, 5);
        evidence_name = evidence_for_type(registry, &resolved_target);
        verifiers = witness_verifiers_for_type(registry, &resolved_target);
        proof_test = type_has_proof_test(proof_chain_subjects, &resolved_target);
    }
    let evidence_link = evidence_name.is_some();
    let kani_witness = verifiers.contains("kani");
    let creusot_witness = verifiers.contains("creusot");
    let verus_witness = verifiers.contains("verus");

    let kani_applicable = exception.is_none_or(|e| !e.covers("kani"));
    let creusot_applicable = exception.is_none_or(|e| !e.covers("creusot"));
    let verus_applicable = exception.is_none_or(|e| !e.covers("verus"));

    let status = if !evidence_link {
        AmenableStdStatus::Missing
    } else if (!kani_applicable || kani_witness)
        && (!creusot_applicable || creusot_witness)
        && (!verus_applicable || verus_witness)
    {
        AmenableStdStatus::Complete
    } else {
        AmenableStdStatus::Partial
    };

    AmenableStdEntry::builder()
        .type_path(type_path.to_string())
        .type_kind(type_kind.to_string())
        .is_generic(is_generic)
        .evidence_link(evidence_link)
        .evidence_name(evidence_name)
        .kani_witness(kani_witness)
        .creusot_witness(creusot_witness)
        .verus_witness(verus_witness)
        .proof_test(proof_test)
        .status(status)
        .skip_reason(exception.map(|e| e.reason().clone()))
        .kani_excepted(!kani_applicable)
        .creusot_excepted(!creusot_applicable)
        .verus_excepted(!verus_applicable)
        .build()
}

/// Classify one std inventory row for amenable registry coverage.
#[instrument(level = "debug", skip(args))]
pub fn classify_amenable_std_row(
    type_path: &str,
    args: ClassifyRowArgs<'_>,
) -> CordialResult<AmenableStdEntry> {
    classify_wrapped_row(
        type_path,
        args,
        evidence_for_std_type,
        witness_verifiers_for_std_type,
        std_type_has_proof_test,
    )
}

/// Classify one third-party (`amenable_ext`) inventory row for amenable
/// registry coverage.
#[instrument(level = "debug", skip(args))]
pub fn classify_amenable_ext_row(
    type_path: &str,
    args: ClassifyRowArgs<'_>,
) -> CordialResult<AmenableStdEntry> {
    classify_wrapped_row(
        type_path,
        args,
        evidence_for_ext_type,
        witness_verifiers_for_ext_type,
        ext_type_has_proof_test,
    )
}

/// Resolve alias chain.
#[instrument(level = "debug", skip(items))]
pub fn resolve_alias_chain(items: &[StdInventoryItem], start: &str, max_hops: usize) -> String {
    let mut current = start.to_string();
    for _ in 0..max_hops {
        let Some(next_item) = items.iter().find(|candidate| {
            candidate.path() == &current || candidate.path().ends_with(&format!("::{current}"))
        }) else {
            break;
        };
        let Some(next_target) = next_item.alias_target() else {
            break;
        };
        if *next_target == current {
            break;
        }
        current = next_target.clone();
    }
    current
}

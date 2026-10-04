#![cfg(feature = "amenable_ext")]

use std::collections::HashSet;

use miette::IntoDiagnostic;

use cordial::testing::{
    AmenableExtOptions, AmenableStdStatus, EvidenceLinkDump, InventoryItemKind, ProofRecordDump,
    RegistryDump, StdInventoryItem, VerifierSkipEntry, VerifierSkipMap, build_amenable_ext_gaps,
    build_amenable_ext_report, evidence_for_ext_type, parse_ext_standard_inner,
    witness_verifiers_for_ext_type,
};

fn sample_item(path: &str) -> StdInventoryItem {
    StdInventoryItem::new(
        path.to_string(),
        InventoryItemKind::Struct,
        false,
        false,
        None,
    )
}

#[test]
fn parse_ext_standard_inner_extracts_type_parameter() {
    cordial::init_tracing();
    assert_eq!(
        parse_ext_standard_inner("amenable_ext::ExtStandard<jiff::timestamp::Timestamp>"),
        Some("jiff::timestamp::Timestamp".to_string())
    );
    // Does not fall back to the RustStdStandard prefix pair -- the two
    // wrapper families are deliberately kept separate.
    assert_eq!(
        parse_ext_standard_inner("amenable_std::rust_std::RustStdStandard<std::string::String>"),
        None
    );
}

#[test]
fn evidence_for_ext_type_matches_generic_instantiation_to_bare_inventory_path() {
    cordial::init_tracing();
    let registry = RegistryDump::new(
        vec![EvidenceLinkDump::new(
            "amenable_ext::ExtStandard<jiff::zoned::Zoned>".to_string(),
            String::new(),
            0,
        )],
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );
    assert_eq!(
        evidence_for_ext_type(&registry, "jiff::zoned::Zoned"),
        Some("amenable_ext::ExtStandard<jiff::zoned::Zoned>".to_string())
    );
}

#[test]
fn build_amenable_ext_report_classifies_complete_partial_and_missing() -> miette::Result<()> {
    cordial::init_tracing();
    let items = vec![
        sample_item("jiff::timestamp::Timestamp"),
        sample_item("jiff::zoned::Zoned"),
        sample_item("jiff::span::Span"),
    ];
    let registry = RegistryDump::new(
        vec![
            EvidenceLinkDump::new(
                "amenable_ext::ExtStandard<Timestamp>".to_string(),
                String::new(),
                0,
            ),
            EvidenceLinkDump::new(
                "amenable_ext::ExtStandard<Zoned>".to_string(),
                String::new(),
                0,
            ),
        ],
        vec![
            ProofRecordDump::new(
                "amenable_ext::ExtStandard<Timestamp>".to_string(),
                "kani".to_string(),
            ),
            ProofRecordDump::new(
                "amenable_ext::ExtStandard<Timestamp>".to_string(),
                "creusot".to_string(),
            ),
            ProofRecordDump::new(
                "amenable_ext::ExtStandard<Timestamp>".to_string(),
                "verus".to_string(),
            ),
            ProofRecordDump::new(
                "amenable_ext::ExtStandard<Zoned>".to_string(),
                "kani".to_string(),
            ),
        ],
        Vec::new(),
        Vec::new(),
    );
    let skip = VerifierSkipMap::new();
    let proof_chain: HashSet<String> = HashSet::from(["ExtStandard<Timestamp>".to_string()]);

    let options = AmenableExtOptions::default();
    assert!(!options.include_nightly());

    let report = build_amenable_ext_report(
        "jiff",
        &items,
        "amenable_ext",
        &registry,
        &skip,
        &proof_chain,
        false,
    )
    .into_diagnostic()?;
    assert_eq!(report.complete_count(), 1);
    assert_eq!(report.partial_count(), 1);
    assert_eq!(report.missing_count(), 1);
    assert_eq!(report.skipped_count(), 0);

    let timestamp = report
        .entries()
        .iter()
        .find(|entry| entry.type_path() == "jiff::timestamp::Timestamp")
        .ok_or_else(|| miette::miette!("Timestamp row"))?;
    assert_eq!(timestamp.status(), AmenableStdStatus::Complete);
    assert!(timestamp.proof_test());

    let span = report
        .entries()
        .iter()
        .find(|entry| entry.type_path() == "jiff::span::Span")
        .ok_or_else(|| miette::miette!("Span row"))?;
    assert_eq!(span.status(), AmenableStdStatus::Missing);

    let verifiers = witness_verifiers_for_ext_type(&registry, "jiff::timestamp::Timestamp");
    assert!(
        verifiers.contains("kani") && verifiers.contains("creusot") && verifiers.contains("verus")
    );

    let gaps = build_amenable_ext_gaps(&report);
    // One gap for the missing Span evidence, one for Zoned's partial witnesses.
    assert_eq!(gaps.len(), 2);
    let span_gap = gaps
        .iter()
        .find(|gap| gap.type_path() == "jiff::span::Span")
        .ok_or_else(|| miette::miette!("Span gap"))?;
    // The action names the ext wrapper, not the std one.
    assert!(span_gap.action().contains("ExtStandard<"));
    assert!(!span_gap.action().contains("RustStdStandard<"));
    Ok(())
}

#[test]
fn a_scoped_exception_only_excepts_its_named_verifier() -> miette::Result<()> {
    cordial::init_tracing();
    let items = vec![sample_item("jiff::civil::datetime::DateTime")];
    let registry = RegistryDump::new(
        vec![EvidenceLinkDump::new(
            "amenable_ext::ExtStandard<DateTime>".to_string(),
            String::new(),
            0,
        )],
        vec![
            ProofRecordDump::new(
                "amenable_ext::ExtStandard<DateTime>".to_string(),
                "kani".to_string(),
            ),
            ProofRecordDump::new(
                "amenable_ext::ExtStandard<DateTime>".to_string(),
                "verus".to_string(),
            ),
        ],
        Vec::new(),
        Vec::new(),
    );
    let mut skip = VerifierSkipMap::new();
    skip.insert(
        "jiff::civil::datetime::DateTime".to_string(),
        VerifierSkipEntry::new(
            "creusot has no jiff coverage yet".to_string(),
            Some(["creusot".to_string()].into_iter().collect()),
        ),
    );

    let report = build_amenable_ext_report(
        "jiff",
        &items,
        "amenable_ext",
        &registry,
        &skip,
        &HashSet::new(),
        false,
    )
    .into_diagnostic()?;

    let entry = report
        .entries()
        .iter()
        .find(|entry| entry.type_path() == "jiff::civil::datetime::DateTime")
        .ok_or_else(|| miette::miette!("DateTime row"))?;
    assert!(entry.kani_witness());
    assert!(entry.verus_witness());
    assert!(!entry.creusot_witness());
    assert!(entry.creusot_excepted());
    assert_eq!(entry.status(), AmenableStdStatus::Complete);
    assert!(build_amenable_ext_gaps(&report).is_empty());
    Ok(())
}

#[test]
fn amenable_ext_plugin_is_registered() -> miette::Result<()> {
    cordial::init_tracing();
    use cordial::{PluginCategory, SessionBuilder, coverage_plugins};
    let session = SessionBuilder::new(".").build();
    let plugins = coverage_plugins();
    let plugin = plugins
        .iter()
        .find(|plugin| plugin.id() == "amenable-ext-coverage")
        .ok_or_else(|| miette::miette!("amenable-ext-coverage plugin registered"))?;
    assert_eq!(plugin.category(), PluginCategory::Coverage);
    assert!(
        plugin
            .etiquettes(&session)
            .iter()
            .any(|etiquette| etiquette.id() == "amenable-ext-jiff")
    );
    Ok(())
}

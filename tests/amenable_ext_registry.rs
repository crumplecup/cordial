#![cfg(feature = "amenable_ext")]

use std::collections::HashSet;

use miette::IntoDiagnostic;

use cordial::testing::{
    AmenableExtOptions, AmenableStdStatus, EvidenceLinkDump, InventoryItemKind, PremiseDump,
    ProofRecordDump, RegistryDump, StdInventoryItem, VerifierSkipEntry, VerifierSkipMap,
    build_amenable_ext_gaps, build_amenable_ext_report, evidence_for_ext_type,
    generic_claims_for_ext_type, parse_ext_generic_inner, parse_ext_standard_inner,
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

/// Step 2 (`docs/planning/amenable-ext-targets-config.md`): with
/// `[[amenable_ext.target]]` entries configured, the plugin must build an
/// etiquette per *enabled* target and skip disabled ones -- not just the
/// single compiled-in `jiff` default `amenable_ext_plugin_is_registered`
/// above exercises. Two real targets plus one disabled target, so the
/// generalization is proven on more than one name.
fn write_amenable_ext_config(workspace: &std::path::Path, body: &str) -> miette::Result<()> {
    use miette::WrapErr;
    std::fs::write(workspace.join("cordial.toml"), body)
        .into_diagnostic()
        .wrap_err("write cordial.toml")
}

#[test]
fn configured_targets_replace_the_default_and_skip_disabled_entries() -> miette::Result<()> {
    use cordial::{AMENABLE_EXT_COVERAGE, Plugin, SessionBuilder};
    use miette::WrapErr;

    cordial::init_tracing();
    let workspace = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("workspace")?;
    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    write_amenable_ext_config(
        workspace.path(),
        r#"
[[amenable_ext.target]]
name = "jiff"

[[amenable_ext.target]]
name = "chrono"

[[amenable_ext.target]]
name = "elm"
enabled = false
"#,
    )?;
    let session = SessionBuilder::new(workspace.path())
        .with_store_home(store.path())
        .build();

    let mut ids: Vec<String> = AMENABLE_EXT_COVERAGE
        .etiquettes(&session)
        .iter()
        .map(|etiquette| etiquette.id().to_string())
        .collect();
    ids.sort();
    assert_eq!(ids, vec!["amenable-ext-chrono", "amenable-ext-jiff"]);
    Ok(())
}

#[test]
fn configured_targets_drive_upstream_dep_coverage_targets_too() -> miette::Result<()> {
    use cordial::{
        AMENABLE_EXT_COVERAGE, Coverage, CoverageTargetKind, NamedRunFilter, SessionBuilder,
    };
    use miette::WrapErr;

    cordial::init_tracing();
    let workspace = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("workspace")?;
    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    write_amenable_ext_config(
        workspace.path(),
        r#"
[[amenable_ext.target]]
name = "jiff"

[[amenable_ext.target]]
name = "chrono"

[[amenable_ext.target]]
name = "elm"
enabled = false
"#,
    )?;
    let session = SessionBuilder::new(workspace.path())
        .with_store_home(store.path())
        .build();
    let filter = NamedRunFilter::all_etiquettes();

    let targets = AMENABLE_EXT_COVERAGE
        .target_provider()
        .coverage_targets(&session, &filter)
        .into_diagnostic()
        .wrap_err("coverage_targets")?;
    let mut upstream: Vec<&str> = targets
        .iter()
        .filter(|target| target.kind() == CoverageTargetKind::UpstreamDep)
        .map(|target| target.crate_name().as_str())
        .collect();
    upstream.sort();
    assert_eq!(upstream, vec!["chrono", "jiff"]);
    Ok(())
}

#[test]
fn coverage_summary_renders_one_section_per_configured_target() -> miette::Result<()> {
    use cordial::{
        AMENABLE_EXT_COVERAGE, NamedRunFilter, SessionBuilder, WorkspaceIr, build_coverage_summary,
    };
    use miette::WrapErr;

    cordial::init_tracing();
    let workspace = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("workspace")?;
    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    write_amenable_ext_config(
        workspace.path(),
        r#"
[[amenable_ext.target]]
name = "jiff"

[[amenable_ext.target]]
name = "chrono"
"#,
    )?;
    let session = SessionBuilder::new(workspace.path())
        .with_store_home(store.path())
        .build();
    let filter = NamedRunFilter::all_etiquettes();
    let registered: Vec<&'static dyn cordial::Plugin> = vec![&AMENABLE_EXT_COVERAGE];

    let summary = build_coverage_summary(
        &registered,
        &[],
        &filter,
        &session,
        &[],
        &WorkspaceIr::default(),
    )
    .into_diagnostic()
    .wrap_err("build_coverage_summary")?;

    let mut ids: Vec<&str> = summary
        .plugins()
        .iter()
        .map(|plugin| plugin.plugin_id().as_str())
        .collect();
    ids.sort();
    assert_eq!(ids, vec!["amenable-ext-chrono", "amenable-ext-jiff"]);
    assert!(
        summary
            .plugins()
            .iter()
            .any(|plugin| plugin.body().contains("No amenable-ext-chrono findings"))
    );
    assert!(
        summary
            .plugins()
            .iter()
            .any(|plugin| plugin.body().contains("No amenable-ext-jiff findings"))
    );
    Ok(())
}

#[test]
fn parse_ext_standard_inner_normalizes_stringify_spacing() {
    cordial::init_tracing();
    assert_eq!(
        parse_ext_standard_inner("amenable_ext::ExtStandard<chrono :: DateTime < chrono :: Utc >>"),
        Some("chrono::DateTime".to_string())
    );
}

#[test]
fn parse_ext_generic_inner_reads_generic_wrapper_only() {
    cordial::init_tracing();
    assert_eq!(
        parse_ext_generic_inner("amenable_ext::ExtGeneric<chrono::DateTime<Tz>>"),
        Some("chrono::DateTime".to_string())
    );
    assert_eq!(
        parse_ext_generic_inner("amenable_ext::ExtStandard<chrono::DateTime<chrono::Utc>>"),
        None
    );
    // A generic name is never mistaken for a concrete claim.
    assert_eq!(
        parse_ext_standard_inner("amenable_ext::ExtGeneric<chrono::DateTime<Tz>>"),
        None
    );
}

#[test]
fn generic_link_round_trips_bounds_and_premises_and_old_dumps_still_load() -> miette::Result<()> {
    cordial::init_tracing();
    let json = r#"{
        "evidence_links": [
            {"name": "amenable_ext::ExtStandard<chrono::Utc>", "basis": "", "index": 0},
            {"name": "amenable_ext::ExtGeneric<chrono::DateTime<Tz>>", "basis": "", "index": 0,
             "bounds": ["chrono::offset::TimeZone"],
             "premises": [{"id": "offset-round-trip", "statement": "s"}]}
        ],
        "proof_records": [], "kani_proofs": []
    }"#;
    let registry: RegistryDump = serde_json::from_str(json).into_diagnostic()?;
    let [concrete, generic] = registry.evidence_links().as_slice() else {
        miette::bail!("expected two links");
    };
    assert!(!concrete.is_generic());
    assert!(concrete.bounds().is_empty());
    assert!(generic.is_generic());
    assert_eq!(generic.bounds(), &["chrono::offset::TimeZone".to_string()]);
    assert_eq!(generic.premises()[0].id(), "offset-round-trip");
    Ok(())
}

#[test]
fn generic_claims_for_ext_type_finds_claim_by_inventory_path() {
    cordial::init_tracing();
    let registry = RegistryDump::new(
        vec![
            EvidenceLinkDump::new(
                "amenable_ext::ExtStandard<chrono::DateTime<chrono::Utc>>".to_string(),
                String::new(),
                0,
            ),
            EvidenceLinkDump::generic(
                "amenable_ext::ExtGeneric<chrono::DateTime<Tz>>".to_string(),
                String::new(),
                0,
                vec!["chrono::offset::TimeZone".to_string()],
                vec![PremiseDump::new("p".to_string(), "s".to_string())],
            ),
        ],
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );
    let claims = generic_claims_for_ext_type(&registry, "chrono::DateTime");
    assert_eq!(claims.len(), 1);
    assert!(claims[0].is_generic());
    assert!(generic_claims_for_ext_type(&registry, "chrono::Utc").is_empty());
}

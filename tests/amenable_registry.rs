#![cfg(feature = "amenable_std")]

use std::collections::HashSet;

use cordial::testing::{
    AmenableStdStatus, EvidenceLinkDump, InventoryItemKind, ProofRecordDump, RegistryDump,
    StdInventoryItem, VerifierSkipEntry, VerifierSkipMap, build_amenable_std_gaps,
    build_amenable_std_report, evidence_for_std_type, parse_rust_std_standard_inner,
    witness_verifiers_for_std_type,
};

fn sample_item(path: &str) -> StdInventoryItem {
    StdInventoryItem {
        path: path.to_string(),
        kind: InventoryItemKind::Struct,
        is_generic: false,
        is_unstable: false,
        alias_target: None,
    }
}

fn sample_alias(path: &str, alias_target: &str) -> StdInventoryItem {
    StdInventoryItem {
        path: path.to_string(),
        kind: InventoryItemKind::TypeAlias,
        is_generic: false,
        is_unstable: false,
        alias_target: Some(alias_target.to_string()),
    }
}

#[test]
fn parse_rust_std_standard_inner_extracts_type_parameter() {
    cordial::init_tracing();
    assert_eq!(
        parse_rust_std_standard_inner(
            "amenable_std::rust_std::RustStdStandard<std::string::String>"
        ),
        Some("std::string::String".to_string())
    );
    assert_eq!(
        parse_rust_std_standard_inner(
            "amenable_std::rust_std::RustStdStandard<std::sync::mpsc::Sender<i32>>"
        ),
        Some("std::sync::mpsc::Sender".to_string())
    );
}

#[test]
fn evidence_for_std_type_matches_generic_instantiation_to_bare_inventory_path() {
    cordial::init_tracing();
    let registry = RegistryDump {
        evidence_links: vec![EvidenceLinkDump {
            name: "amenable_std::rust_std::RustStdStandard<std::sync::mpsc::Sender<i32>>"
                .to_string(),
            basis: String::new(),
            index: 0,
        }],
        proof_records: Vec::new(),
        contract_records: Vec::new(),
        kani_proofs: Vec::new(),
    };
    assert_eq!(
        evidence_for_std_type(&registry, "std::sync::mpsc::Sender"),
        Some("amenable_std::rust_std::RustStdStandard<std::sync::mpsc::Sender<i32>>".to_string())
    );
}

#[test]
fn build_amenable_std_report_classifies_complete_partial_missing_and_skipped() -> miette::Result<()>
{
    cordial::init_tracing();
    let items = vec![
        sample_item("std::string::String"),
        sample_item("std::vec::Vec"),
        sample_item("core::fmt::Debug"),
    ];
    let registry = RegistryDump {
        evidence_links: vec![
            EvidenceLinkDump {
                name: "amenable_std::rust_std::RustStdStandard<String>".to_string(),
                basis: String::new(),
                index: 0,
            },
            EvidenceLinkDump {
                name: "amenable_std::rust_std::RustStdStandard<Vec>".to_string(),
                basis: String::new(),
                index: 0,
            },
        ],
        proof_records: vec![
            ProofRecordDump {
                evidence: "amenable_std::rust_std::RustStdStandard<String>".to_string(),
                verifier: "kani".to_string(),
            },
            ProofRecordDump {
                evidence: "amenable_std::rust_std::RustStdStandard<String>".to_string(),
                verifier: "creusot".to_string(),
            },
            ProofRecordDump {
                evidence: "amenable_std::rust_std::RustStdStandard<String>".to_string(),
                verifier: "verus".to_string(),
            },
            ProofRecordDump {
                evidence: "amenable_std::rust_std::RustStdStandard<Vec>".to_string(),
                verifier: "kani".to_string(),
            },
        ],
        contract_records: Vec::new(),
        kani_proofs: Vec::new(),
    };
    let mut skip = VerifierSkipMap::new();
    skip.insert(
        "core::fmt::Debug".to_string(),
        VerifierSkipEntry {
            reason: "trait".to_string(),
            verifiers: None,
        },
    );
    let proof_chain: HashSet<String> = HashSet::from(["RustStdStandard<String>".to_string()]);

    let report = build_amenable_std_report(
        "std",
        &items,
        "amenable_std",
        &registry,
        &skip,
        &proof_chain,
        false,
    );
    assert_eq!(report.complete_count, 1);
    assert_eq!(report.partial_count, 1);
    assert_eq!(report.missing_count, 0);
    assert_eq!(report.skipped_count, 1);

    let string = report
        .entries
        .iter()
        .find(|entry| entry.type_path == "std::string::String")
        .ok_or_else(|| miette::miette!("String row"))?;
    assert_eq!(string.status, AmenableStdStatus::Complete);
    assert!(string.proof_test);

    let vec = report
        .entries
        .iter()
        .find(|entry| entry.type_path == "std::vec::Vec")
        .ok_or_else(|| miette::miette!("Vec row"))?;
    assert_eq!(vec.status, AmenableStdStatus::Partial);

    let gaps = build_amenable_std_gaps(&report);
    assert_eq!(gaps.len(), 1);
    assert_eq!(gaps[0].type_path, "std::vec::Vec");
    Ok(())
}

#[test]
fn witness_verifiers_for_std_type_tolerates_evidence_and_proof_path_drift() {
    cordial::init_tracing();
    let registry = RegistryDump {
        evidence_links: vec![EvidenceLinkDump {
            name: "amenable_std::rust_std::RustStdStandard<std::fmt::Alignment>".to_string(),
            basis: String::new(),
            index: 0,
        }],
        proof_records: vec![ProofRecordDump {
            evidence: "amenable_std::rust_std::RustStdStandard<core::fmt::Alignment>".to_string(),
            verifier: "kani".to_string(),
        }],
        contract_records: Vec::new(),
        kani_proofs: Vec::new(),
    };

    assert!(evidence_for_std_type(&registry, "core::fmt::Alignment").is_some());
    let verifiers = witness_verifiers_for_std_type(&registry, "core::fmt::Alignment");
    assert!(verifiers.contains("kani"));
}

#[test]
fn build_amenable_std_report_resolves_evidence_via_type_alias_target() -> miette::Result<()> {
    cordial::init_tracing();
    let items = vec![sample_alias("core::num::NonZeroI8", "NonZero")];
    let registry = RegistryDump {
        evidence_links: vec![EvidenceLinkDump {
            name: "amenable_std::rust_std::RustStdStandard<NonZero<i8>>".to_string(),
            basis: String::new(),
            index: 0,
        }],
        proof_records: vec![ProofRecordDump {
            evidence: "amenable_std::rust_std::RustStdStandard<NonZero<i8>>".to_string(),
            verifier: "kani".to_string(),
        }],
        contract_records: Vec::new(),
        kani_proofs: Vec::new(),
    };
    let skip = VerifierSkipMap::new();
    let proof_chain: HashSet<String> = HashSet::new();

    let report = build_amenable_std_report(
        "std",
        &items,
        "amenable_std",
        &registry,
        &skip,
        &proof_chain,
        false,
    );

    let nonzero_i8 = report
        .entries
        .iter()
        .find(|entry| entry.type_path == "core::num::NonZeroI8")
        .ok_or_else(|| miette::miette!("NonZeroI8 row"))?;
    assert!(nonzero_i8.evidence_link);
    assert!(nonzero_i8.kani_witness);
    assert_eq!(nonzero_i8.status, AmenableStdStatus::Partial);
    Ok(())
}

#[test]
fn a_scoped_exception_only_excepts_its_named_verifier_and_keeps_real_witnesses_visible()
-> miette::Result<()> {
    cordial::init_tracing();
    let items = vec![sample_item("std::os::windows::ffi::EncodeWide")];
    let registry = RegistryDump {
        evidence_links: vec![EvidenceLinkDump {
            name: "amenable_std::rust_std::RustStdStandard<EncodeWide>".to_string(),
            basis: String::new(),
            index: 0,
        }],
        proof_records: vec![
            ProofRecordDump {
                evidence: "amenable_std::rust_std::RustStdStandard<EncodeWide>".to_string(),
                verifier: "kani".to_string(),
            },
            ProofRecordDump {
                evidence: "amenable_std::rust_std::RustStdStandard<EncodeWide>".to_string(),
                verifier: "verus".to_string(),
            },
        ],
        contract_records: Vec::new(),
        kani_proofs: Vec::new(),
    };
    let mut skip = VerifierSkipMap::new();
    skip.insert(
        "std::os::windows::ffi::EncodeWide".to_string(),
        VerifierSkipEntry {
            reason: "creusot has no Windows target".to_string(),
            verifiers: Some(["creusot".to_string()].into_iter().collect()),
        },
    );

    let report = build_amenable_std_report(
        "std",
        &items,
        "amenable_std",
        &registry,
        &skip,
        &HashSet::new(),
        false,
    );

    assert_eq!(report.skipped_count, 0);
    let entry = report
        .entries
        .iter()
        .find(|entry| entry.type_path == "std::os::windows::ffi::EncodeWide")
        .ok_or_else(|| miette::miette!("EncodeWide row"))?;
    assert!(entry.kani_witness);
    assert!(entry.verus_witness);
    assert!(!entry.creusot_witness);
    assert!(entry.creusot_excepted);
    assert_eq!(entry.status, AmenableStdStatus::Complete);
    assert!(build_amenable_std_gaps(&report).is_empty());
    Ok(())
}

#[test]
fn a_scoped_exception_does_not_hide_a_real_gap_on_a_different_verifier() -> miette::Result<()> {
    cordial::init_tracing();
    let items = vec![sample_item("std::os::windows::prelude::OwnedSocket")];
    let registry = RegistryDump {
        evidence_links: vec![EvidenceLinkDump {
            name: "amenable_std::rust_std::RustStdStandard<OwnedSocket>".to_string(),
            basis: String::new(),
            index: 0,
        }],
        proof_records: vec![ProofRecordDump {
            evidence: "amenable_std::rust_std::RustStdStandard<OwnedSocket>".to_string(),
            verifier: "kani".to_string(),
        }],
        contract_records: Vec::new(),
        kani_proofs: Vec::new(),
    };
    let mut skip = VerifierSkipMap::new();
    skip.insert(
        "std::os::windows::prelude::OwnedSocket".to_string(),
        VerifierSkipEntry {
            reason: "creusot has no Windows target".to_string(),
            verifiers: Some(["creusot".to_string()].into_iter().collect()),
        },
    );

    let report = build_amenable_std_report(
        "std",
        &items,
        "amenable_std",
        &registry,
        &skip,
        &HashSet::new(),
        false,
    );

    let entry = report
        .entries
        .iter()
        .find(|entry| entry.type_path == "std::os::windows::prelude::OwnedSocket")
        .ok_or_else(|| miette::miette!("OwnedSocket row"))?;
    assert_eq!(entry.status, AmenableStdStatus::Partial);

    let gaps = build_amenable_std_gaps(&report);
    assert_eq!(gaps.len(), 1);
    assert_eq!(gaps[0].missing_layers, "verus_witness, proof_test");
    Ok(())
}

#[test]
fn amenable_std_plugin_is_registered() -> miette::Result<()> {
    cordial::init_tracing();
    use cordial::{PluginCategory, coverage_plugins};
    let plugins = coverage_plugins();
    assert!(
        plugins
            .iter()
            .any(|plugin| plugin.id() == "amenable-std-coverage")
    );
    assert_eq!(
        plugins
            .iter()
            .find(|plugin| plugin.id() == "amenable-std-coverage")
            .ok_or_else(|| miette::miette!("plugin"))?
            .category(),
        PluginCategory::Coverage
    );
    Ok(())
}

#[test]
fn follows_a_two_hop_alias_chain_to_the_concrete_target() {
    cordial::init_tracing();
    use cordial::testing::{InventoryItemKind, StdInventoryItem, resolve_alias_chain};

    let items = vec![StdInventoryItem {
        path: "std::os::windows::raw::HANDLE".to_string(),
        kind: InventoryItemKind::TypeAlias,
        is_generic: false,
        is_unstable: false,
        alias_target: Some("*mut crate::os::raw::c_void".to_string()),
    }];
    assert_eq!(
        resolve_alias_chain(&items, "raw::HANDLE", 5),
        "*mut crate::os::raw::c_void"
    );
}

#[test]
fn stops_on_a_self_referential_cycle() {
    cordial::init_tracing();
    use cordial::testing::{InventoryItemKind, StdInventoryItem, resolve_alias_chain};

    let items = vec![StdInventoryItem {
        path: "a::b".to_string(),
        kind: InventoryItemKind::TypeAlias,
        is_generic: false,
        is_unstable: false,
        alias_target: Some("b".to_string()),
    }];
    assert_eq!(resolve_alias_chain(&items, "b", 5), "b");
}

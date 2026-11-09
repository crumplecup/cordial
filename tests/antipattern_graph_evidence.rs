//! Antipattern policy and registry evaluation must use persisted graph facts.
#![cfg(feature = "antipatterns")]

use cordial::AntipatternWorkspaceFacts;
use cordial::{
    ATTR_SOURCE_ANTIPATTERNS, AntipatternRuleId, ContractRecordDump, CrateIr, CrateTarget,
    ErrorFieldStrategy, IrView, LoadContext, Loader, SessionBuilder, SourceAntipatterns,
    SourceLoadView, SourceLoader, StaticRefStrategy, scan_crate_contract_bounds,
};
use miette::{IntoDiagnostic, Result};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, path::Path};

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn policy_changes_replay_from_required_graph_evidence() -> Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic()?;
    let root = fixture.path();
    std::fs::create_dir_all(root.join("src")).into_diagnostic()?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        root.join("src/lib.rs"),
        include_str!("fixtures/quality/antipatterns/graph_policy.rs"),
    )
    .into_diagnostic()?;
    std::fs::write(root.join("src/empty.rs"), "// empty\n").into_diagnostic()?;
    let graph = load(root, "policy")?;
    let evidence = graph.source_antipatterns().into_diagnostic()?;
    assert_eq!(evidence.snapshots().len(), 2);
    for snapshot in evidence.snapshots() {
        let digest: [u8; 32] =
            Sha256::digest(std::fs::read(snapshot.path()).into_diagnostic()?).into();
        assert_eq!(snapshot.digest(), digest);
    }
    let encoded = serde_json::to_value(&evidence).into_diagnostic()?;
    let restored: SourceAntipatterns = serde_json::from_value(encoded.clone()).into_diagnostic()?;
    assert_eq!(restored, evidence);
    let mut incomplete = encoded;
    incomplete
        .as_object_mut()
        .ok_or_else(|| miette::miette!("object"))?
        .remove("catalog");
    assert!(serde_json::from_value::<SourceAntipatterns>(incomplete).is_err());
    assert!(CrateIr::new("missing").source_antipatterns().is_err());
    let mut corrupt = graph.clone();
    corrupt
        .set_attr(
            corrupt.root(),
            ATTR_SOURCE_ANTIPATTERNS,
            serde_json::json!({}),
        )
        .into_diagnostic()?;
    assert!(corrupt.source_antipatterns().is_err());

    let facts = AntipatternWorkspaceFacts::new(Vec::new(), HashMap::new());
    let mut expected = Vec::new();
    for static_strategy in [
        StaticRefStrategy::String,
        StaticRefStrategy::Cow,
        StaticRefStrategy::Const,
    ] {
        for error_strategy in [ErrorFieldStrategy::Owned, ErrorFieldStrategy::Borrowed] {
            let records = evidence
                .assess("policy", &facts, static_strategy, error_strategy)
                .into_diagnostic()?;
            let fields: Vec<_> = records
                .iter()
                .filter(|row| row.rule_id() == AntipatternRuleId::StructStaticRef001)
                .collect();
            assert_eq!(
                fields.len(),
                if error_strategy == ErrorFieldStrategy::Borrowed {
                    2
                } else {
                    4
                }
            );
            assert!(
                fields
                    .iter()
                    .any(|row| row.snippet().contains("copy `file` and `line`"))
            );
            let ordinary = fields
                .iter()
                .find(|row| row.context().contains("Ordinary"))
                .ok_or_else(|| miette::miette!("ordinary field"))?;
            assert!(ordinary.snippet().contains(match static_strategy {
                StaticRefStrategy::String => "owned String",
                StaticRefStrategy::Cow => "Cow<'static, str>",
                StaticRefStrategy::Const => "const/static only",
            }));
            expected.push((static_strategy, error_strategy, records));
        }
    }
    std::fs::remove_dir_all(root.join("src")).into_diagnostic()?;
    for (static_strategy, error_strategy, records) in expected {
        assert_eq!(
            restored
                .assess("policy", &facts, static_strategy, error_strategy)
                .into_diagnostic()?,
            records,
            "complete rows must survive deletion and policy changes"
        );
    }
    Ok(())
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn registry_changes_replay_saved_contract_shapes_for_each_verifier() -> Result<()> {
    cordial::init_tracing();
    for (verifier, text) in [
        (
            "creusot",
            include_str!("fixtures/quality/antipatterns/contract_bounds_creusot.rs"),
        ),
        (
            "kani",
            include_str!("fixtures/quality/antipatterns/contract_bounds_kani.rs"),
        ),
        (
            "verus",
            include_str!("fixtures/quality/antipatterns/contract_bounds_verus.rs"),
        ),
    ] {
        let fixture = tempfile::tempdir().into_diagnostic()?;
        let root = fixture.path();
        std::fs::create_dir_all(root.join("src")).into_diagnostic()?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
        )
        .into_diagnostic()?;
        std::fs::write(root.join("src/lib.rs"), text).into_diagnostic()?;
        let name = format!("amenable_{verifier}");
        let evidence = load(root, &name)?.source_antipatterns().into_diagnostic()?;
        let registry = registry(verifier)?;
        let mut outcomes = Vec::new();
        for records in [Vec::new(), registry] {
            let expected = scan_crate_contract_bounds(root, &name, &records).into_diagnostic()?;
            assert_eq!(
                expected.len(),
                match (verifier, records.is_empty()) {
                    ("creusot", true) => 3,
                    ("kani", true) => 3,
                    ("verus", true) => 2,
                    ("creusot", false) => 2,
                    _ => 1,
                }
            );
            outcomes.push((
                AntipatternWorkspaceFacts::new(records, HashMap::new()),
                expected,
            ));
        }
        let evidence: SourceAntipatterns =
            serde_json::from_value(serde_json::to_value(evidence).into_diagnostic()?)
                .into_diagnostic()?;
        std::fs::remove_dir_all(root.join("src")).into_diagnostic()?;
        for (facts, expected) in outcomes {
            let actual: Vec<_> = evidence
                .assess(
                    &name,
                    &facts,
                    StaticRefStrategy::String,
                    ErrorFieldStrategy::Owned,
                )
                .into_diagnostic()?
                .into_iter()
                .filter(|row| row.rule_id() == AntipatternRuleId::UnnamedContractBound001)
                .collect();
            assert_eq!(
                actual, expected,
                "{verifier}: full source locations and snippets"
            );
        }
    }
    Ok(())
}

#[tracing::instrument(level = "debug", err(level = "warn"))]
fn registry(verifier: &str) -> Result<Vec<ContractRecordDump>> {
    let mut records = Vec::new();
    for kind in ["requires", "ensures"] {
        records.push(
            ContractRecordDump::builder()
                .evidence("fixture::NonNegative".into())
                .verifier(verifier.into())
                .kind(kind.into())
                .fragment("fn char_roundtrips(c: char) -> bool { true }".into())
                .build()
                .into_diagnostic()?,
        );
    }
    Ok(records)
}

#[tracing::instrument(level = "debug", skip(root), err(level = "warn"))]
fn load(root: &Path, name: &str) -> Result<CrateIr> {
    let session = SessionBuilder::new(root).build();
    let target = CrateTarget::new(name, root);
    let loaded = SourceLoader
        .load(LoadContext::new(&session, &target))
        .into_diagnostic()?;
    let source = loaded
        .as_any()
        .downcast_ref::<SourceLoadView>()
        .ok_or_else(|| miette::miette!("source view"))?;
    let mut graph = CrateIr::new(name);
    source.populate_ir(&mut graph).into_diagnostic()?;
    Ok(graph)
}

//! Proof reachability uses complete loader evidence, independent of policy or disk.
#![cfg(feature = "tracing")]
// Slow (over 60s): compiled only without the default `quick` feature; run with `just test-slow`.
#![cfg(not(feature = "quick"))]

use cordial::{CrateView, IrView, SourceDatabase, TracingThresholds, WorkspaceIr};
use miette::{IntoDiagnostic, Result};
use std::collections::BTreeSet;

const LEAF: &str = include_str!("fixtures/tracing_proof_calls/leaf.rs");
const PROOFS: &str = include_str!("fixtures/tracing_proof_calls/proofs.rs");

#[tracing::instrument(level = "debug", err(level = "warn"))]
fn fixture() -> Result<tempfile::TempDir> {
    let fixture = tempfile::tempdir().into_diagnostic()?;
    let root = fixture.path();
    std::fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"leaf\", \"proofs\", \"facade\"]\nresolver = \"2\"\n",
    )
    .into_diagnostic()?;
    for (name, dependencies, source) in [
        ("leaf", "", LEAF),
        ("proofs", "leaf = { path = \"../leaf\" }", PROOFS),
        ("facade", "proofs = { path = \"../proofs\" }", ""),
    ] {
        let member = root.join(name);
        std::fs::create_dir_all(member.join("src")).into_diagnostic()?;
        std::fs::write(member.join("Cargo.toml"), format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[dependencies]\n{dependencies}\n")).into_diagnostic()?;
        std::fs::write(member.join("src/lib.rs"), source).into_diagnostic()?;
    }
    Ok(fixture)
}

#[tracing::instrument(level = "debug", skip(facts), err(level = "warn"))]
fn exclusions(facts: &cordial::PathInclusionFacts, cfg: &str) -> Result<Vec<BTreeSet<String>>> {
    let config: TracingThresholds =
        toml::from_str(&format!("[apply_gate_crates]\nfacade = \"{cfg}\"\n")).into_diagnostic()?;
    let mut workspace = WorkspaceIr::default();
    workspace.set_path_inclusions(facts.clone());
    workspace
        .refresh_tracing_call_graph(&config)
        .into_diagnostic()?;
    ["leaf", "proofs", "facade"]
        .into_iter()
        .map(|name| {
            let view = CrateView::new(&workspace, name.into());
            let graph = view
                .workspace_call_graph()
                .ok_or_else(|| miette::miette!("missing call graph"))?;
            Ok(graph.never_instrument(name).iter().cloned().collect())
        })
        .collect()
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn canonical_proof_evidence_matches_and_reachability_uses_only_ir() -> Result<()> {
    cordial::init_tracing();
    let fixture = fixture()?;
    let root = fixture.path();
    let legacy = cordial::workspace_path_inclusions(root).into_diagnostic()?;
    let mut database = SourceDatabase::load(root).into_diagnostic()?;
    let native = database.project_path_inclusions().into_diagnostic()?;
    assert_eq!(
        legacy, native,
        "entire workspace payload, not only function IDs"
    );
    let expected = vec![
        BTreeSet::from_iter(
            [
                "leaf",
                "bridge",
                "assertion_target",
                "Widget<u8>::ensures",
                "Widget<u16>::seeded",
                "<Widget<u32> as Contract>::check",
            ]
            .map(String::from),
        ),
        BTreeSet::from_iter(
            [
                "proofs::harness",
                "macro_proof",
                "nested::nested_macro_proof",
            ]
            .map(String::from),
        ),
        BTreeSet::new(),
    ];
    assert_eq!(exclusions(&native, "kani")?, expected);
    assert_eq!(exclusions(&legacy, "kani")?, expected);
    let other = exclusions(&native, "other")?;
    assert_eq!(other[0], BTreeSet::from(["message_only".into()]));
    assert_eq!(other[1], BTreeSet::from(["inactive::dormant".into()]));

    let serialized = serde_json::to_value(&native).into_diagnostic()?;
    let leaf_index = native
        .sources()
        .iter()
        .position(|source| source.crate_name() == "leaf")
        .ok_or_else(|| miette::miette!("missing leaf evidence"))?;
    let restored: cordial::PathInclusionFacts =
        serde_json::from_value(serialized.clone()).into_diagnostic()?;
    assert_eq!(restored, native);
    for field in [
        "qualified_name",
        "call_keys",
        "cfg_names",
        "attribute_names",
        "calls",
    ] {
        let mut corrupt = serialized.clone();
        corrupt["sources"][leaf_index]["proof_calls"][0]
            .as_object_mut()
            .ok_or_else(|| miette::miette!("missing proof function"))?
            .remove(field);
        assert!(
            serde_json::from_value::<cordial::PathInclusionFacts>(corrupt).is_err(),
            "required field {field}"
        );
    }
    let mut corrupt = serialized;
    corrupt["sources"][0]["proof_calls"] = serde_json::json!(false);
    assert!(serde_json::from_value::<cordial::PathInclusionFacts>(corrupt).is_err());
    assert!(
        WorkspaceIr::default()
            .refresh_tracing_call_graph(&TracingThresholds::default())
            .is_err()
    );

    let proofs = root.join("proofs/src/lib.rs");
    database
        .change_file(
            &proofs,
            PROOFS.replace("assertion_target()", "opaque_only()"),
        )
        .into_diagnostic()?;
    let changed_calls = database.project_path_inclusions().into_diagnostic()?;
    for (before, after) in native.sources().iter().zip(changed_calls.sources()) {
        assert_eq!(
            before
                .proof_calls()
                .iter()
                .map(|function| function.qualified_name())
                .collect::<Vec<_>>(),
            after
                .proof_calls()
                .iter()
                .map(|function| function.qualified_name())
                .collect::<Vec<_>>()
        );
    }
    let changed_exclusions = exclusions(&changed_calls, "kani")?;
    assert!(
        !changed_exclusions[0].contains("assertion_target"),
        "same identities cannot conceal changed calls"
    );
    assert!(
        !changed_exclusions[0].contains("opaque_only"),
        "ordinary caller still wins"
    );
    database
        .change_file(
            &proofs,
            format!("{PROOFS}\nfn ordinary() {{ bridge(); }}\n"),
        )
        .into_diagnostic()?;
    let revised = database.project_path_inclusions().into_diagnostic()?;
    assert_ne!(revised, native);
    let revised_exclusions = exclusions(&revised, "kani")?;
    assert!(
        !revised_exclusions[0].contains("bridge"),
        "ordinary caller prevents proof exclusion"
    );
    assert!(
        !revised_exclusions[0].contains("leaf"),
        "ordinary reachability propagates through bridge"
    );
    assert!(
        revised_exclusions[0].contains("assertion_target"),
        "independent proof calls remain"
    );
    assert_eq!(std::fs::read_to_string(&proofs).into_diagnostic()?, PROOFS);
    database
        .change_file(&proofs, PROOFS.into())
        .into_diagnostic()?;
    assert_eq!(
        database.project_path_inclusions().into_diagnostic()?,
        native
    );
    database
        .change_file(&proofs, "mod {".into())
        .into_diagnostic()?;
    assert!(database.project_path_inclusions().is_err());

    // Re-evaluation must need neither source files nor a live semantic database.
    drop(database);
    std::fs::remove_dir_all(root.join("leaf/src")).into_diagnostic()?;
    std::fs::remove_dir_all(root.join("proofs/src")).into_diagnostic()?;
    assert_eq!(exclusions(&restored, "kani")?, expected);
    Ok(())
}

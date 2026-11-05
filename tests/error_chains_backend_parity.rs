//! Loader-owned chain records survive without source files or parse carriers.
#![cfg(feature = "error_chain")]

#[path = "support/error_sites_projection.rs"]
mod projection;

use cordial::{ATTR_SOURCE_ERROR_CHAINS, CrateIr, CrateTarget, ErrorChainProbeId, IrView};
use miette::{IntoDiagnostic, Result};

const LIB: &str = r#"
mod child;
struct Wrapper { source: std::io::Error }
enum ErrorKind { Io(Wrapper), Text(String) }
impl From<std::io::Error> for Wrapper {
    fn from(source: std::io::Error) -> Self { Self { source } }
}
fn read() -> Result<(), Wrapper> {
    std::fs::read("x")?;
    std::fs::read("x").map_err(Wrapper::from)?;
    Ok(())
}
#[cfg(test)] mod excluded { struct Hidden { source: std::io::Error } }
"#;

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn loader_chain_inventory_replays_without_original_inputs() -> Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic()?;
    let root = fixture.path();
    std::fs::create_dir_all(root.join("src")).into_diagnostic()?;
    std::fs::create_dir_all(root.join("tests")).into_diagnostic()?;
    std::fs::write(root.join("src/lib.rs"), LIB).into_diagnostic()?;
    std::fs::write(root.join("src/child.rs"), "// empty coverage\n").into_diagnostic()?;
    std::fs::write(
        root.join("tests/ignored.rs"),
        "struct Test { source: std::io::Error }",
    )
    .into_diagnostic()?;
    let target = CrateTarget::new("chain_replay", root);
    let graph = projection::baseline(root, &target)?;
    let inventory = graph.source_error_chains().into_diagnostic()?;
    assert_eq!(inventory.files().len(), 2);
    assert_eq!(
        inventory.records(),
        cordial::scan_crate_error_chain(root).into_diagnostic()?
    );
    let records = inventory.records();
    assert_eq!(records.len(), 5);
    for rule in [
        ErrorChainProbeId::WrapperSourceField001,
        ErrorChainProbeId::KindWrapperPayload001,
        ErrorChainProbeId::FromBridge001,
        ErrorChainProbeId::PreservedQuestionMark001,
        ErrorChainProbeId::PreservedMapErr001,
    ] {
        assert!(
            records.iter().any(|row| row.rule_id() == rule),
            "missing {rule}"
        );
    }
    let hooks = &["error-site-inventory", "error-chain-inventory"];
    let expected = projection::materialized_with(&graph, root, hooks, "error_chain_rule_id")?;
    assert_eq!(
        expected.len(),
        6,
        "map_err and try nodes share the located chain annotation"
    );
    assert!(!projection::materialized(&graph, root)?.is_empty());
    let value = serde_json::to_value(&inventory).into_diagnostic()?;
    for key in ["rule_id", "context", "file", "line", "snippet"] {
        let mut broken = value.clone();
        let index = inventory
            .files()
            .iter()
            .position(|file| !file.records().is_empty())
            .ok_or_else(|| miette::miette!("chain record"))?;
        broken["files"][index]["records"][0]
            .as_object_mut()
            .ok_or_else(|| miette::miette!("chain payload"))?
            .remove(key);
        let mut corrupt = graph.clone();
        corrupt
            .set_attr(corrupt.root(), ATTR_SOURCE_ERROR_CHAINS, broken)
            .into_diagnostic()?;
        assert!(corrupt.source_error_chains().is_err(), "required {key}");
    }
    assert!(CrateIr::new("empty").source_error_chains().is_err());
    std::fs::remove_dir_all(root.join("src")).into_diagnostic()?;
    assert_eq!(graph.source_error_chains().into_diagnostic()?, inventory);
    assert_eq!(
        projection::materialized_with(&graph, root, hooks, "error_chain_rule_id")?,
        expected
    );
    Ok(())
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn canonical_chain_payloads_follow_virtual_revisions() -> Result<()> {
    cordial::init_tracing();
    let _subscriber = tracing::subscriber::set_default(tracing_subscriber::registry());
    let fixture = tempfile::tempdir().into_diagnostic()?;
    let root = fixture.path();
    std::fs::create_dir_all(root.join("tests/fixtures")).into_diagnostic()?;
    std::fs::create_dir_all(root.join("src")).into_diagnostic()?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname=\"chain_parity\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[workspace]\n",
    )
    .into_diagnostic()?;
    let source = include_str!("fixtures/semantic_backend/error_chains.rs.txt");
    let library = root.join("src/lib.rs");
    std::fs::write(&library, source).into_diagnostic()?;
    std::fs::write(
        root.join("src/child.rs"),
        "pub fn leaf() -> Result<(), Error> { std::fs::read(\"x\")?; Ok(()) }\n",
    )
    .into_diagnostic()?;
    std::fs::write(root.join("src/empty.rs"), "// no chain records\n").into_diagnostic()?;
    std::fs::write(root.join("tests/fixtures/ignored.rs"), "fn invalid( {").into_diagnostic()?;
    let target = CrateTarget::new("chain_parity", root);
    let baseline = projection::baseline(root, &target)?;
    let expected = baseline.source_error_chains().into_diagnostic()?;
    assert_eq!(expected.files().len(), 3);
    let mut database = cordial::SourceDatabase::load(root).into_diagnostic()?;
    let native = database.project(&target, &[]).into_diagnostic()?;
    let actual = native.source_error_chains().into_diagnostic()?;
    assert_eq!(
        actual, expected,
        "complete records, snapshots and empty-file coverage"
    );
    let hooks = &["error-site-inventory", "error-chain-inventory"];
    let rows = projection::materialized_with(&native, root, hooks, "error_chain_rule_id")?;
    assert_eq!(
        rows,
        projection::materialized_with(&baseline, root, hooks, "error_chain_rule_id")?
    );
    for context in [
        "child::leaf",
        "dormant::inactive",
        "Wrapper::method",
        "read::inner",
        "inside::read::nested",
    ] {
        assert!(
            actual
                .records()
                .iter()
                .any(|record| record.context() == context),
            "missing {context}"
        );
    }
    assert!(
        !actual
            .records()
            .iter()
            .any(|record| record.context().contains("excluded"))
    );
    database
        .change_file(
            &library,
            source.replace("std::fs::read(\"x\")?;", "serde_json::from_str(\"x\")?;"),
        )
        .into_diagnostic()?;
    let changed = database
        .project(&target, &[])
        .into_diagnostic()?
        .source_error_chains()
        .into_diagnostic()?;
    assert_ne!(changed, actual);
    assert!(
        changed
            .records()
            .iter()
            .any(|row| row.foreign_error_type().as_deref() == Some("serde_json::Error"))
    );
    assert_eq!(std::fs::read_to_string(&library).into_diagnostic()?, source);
    database
        .change_file(&library, source.into())
        .into_diagnostic()?;
    assert_eq!(
        database
            .project(&target, &[])
            .into_diagnostic()?
            .source_error_chains()
            .into_diagnostic()?,
        actual
    );
    database
        .change_file(&root.join("src/empty.rs"), "fn invalid( {".into())
        .into_diagnostic()?;
    assert!(
        database.project(&target, &[]).is_err(),
        "invalid inventory-only inputs fail closed"
    );
    drop(database);
    std::fs::remove_dir_all(root.join("src")).into_diagnostic()?;
    assert_eq!(native.source_error_chains().into_diagnostic()?, actual);
    assert_eq!(
        projection::materialized_with(&native, root, hooks, "error_chain_rule_id")?,
        rows
    );
    Ok(())
}

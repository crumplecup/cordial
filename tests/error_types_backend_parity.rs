//! Type graph contracts preserve raw evidence, revisions and source-free replay.
#![cfg(feature = "internal_error_chain")]

#[path = "support/error_sites_projection.rs"]
mod projection;

use cordial::{ATTR_SOURCE_ERROR_TYPES, CrateIr, CrateTarget, IrView};
use miette::{IntoDiagnostic, Result};

const HOOKS: &[&str] = &["error-type-inventory"];
const ATTR: &str = "internal_error_record_kind";
const CHILD: &str =
    "impl std::error::Error for CrossFile {}\nstruct UnrelatedChild { source: std::io::Error }\n";

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn loader_type_inventory_is_required_and_replays_without_source() -> Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic()?;
    let root = fixture.path();
    std::fs::create_dir_all(root.join("src")).into_diagnostic()?;
    std::fs::write(
        root.join("src/lib.rs"),
        "struct CrossFile { source: std::io::Error }\nstruct Ordinary { source: std::io::Error }",
    )
    .into_diagnostic()?;
    std::fs::write(root.join("src/child.rs"), CHILD).into_diagnostic()?;
    std::fs::write(root.join("src/empty.rs"), "// no types\n").into_diagnostic()?;
    let target = CrateTarget::new("type_replay", root);
    let graph = projection::baseline(root, &target)?;
    let inventory = graph.source_error_types().into_diagnostic()?;
    assert_eq!(inventory.files().len(), 3);
    let report = inventory.report("type_replay", false).into_diagnostic()?;
    assert_eq!(report.nodes().len(), 1, "cross-file implementor filtering");
    assert_eq!(report.nodes()[0].type_path(), "CrossFile");
    assert_eq!(
        report,
        *cordial::scan_crate_internal_error_chain(root, "type_replay")
            .into_diagnostic()?
            .type_graph()
    );
    let expected = projection::materialized_with(&graph, root, HOOKS, ATTR)?;
    assert_eq!(expected.len(), 1);
    let value = serde_json::to_value(&inventory).into_diagnostic()?;
    let index = inventory
        .files()
        .iter()
        .position(|file| !file.nodes().is_empty())
        .ok_or_else(|| miette::miette!("raw relationships"))?;
    for (container, keys) in [
        ("file", vec!["snapshot", "nodes", "error_impls"]),
        (
            "node",
            vec!["type_path", "probe_id", "file", "line", "snippet"],
        ),
    ] {
        for key in keys {
            let mut broken = value.clone();
            let slot = if container == "file" {
                &mut broken["files"][index]
            } else {
                &mut broken["files"][index]["nodes"][0]
            };
            slot.as_object_mut()
                .ok_or_else(|| miette::miette!("inventory object"))?
                .remove(key);
            let mut corrupt = graph.clone();
            corrupt
                .set_attr(corrupt.root(), ATTR_SOURCE_ERROR_TYPES, broken)
                .into_diagnostic()?;
            assert!(corrupt.source_error_types().is_err(), "required {key}");
        }
    }
    assert!(CrateIr::new("empty").source_error_types().is_err());
    std::fs::remove_dir_all(root.join("src")).into_diagnostic()?;
    assert_eq!(
        inventory.report("type_replay", false).into_diagnostic()?,
        report
    );
    assert_eq!(
        projection::materialized_with(&graph, root, HOOKS, ATTR)?,
        expected
    );
    Ok(())
}

// Slow (over 60s): compiled only without the default `quick` feature; run with `just test-slow`.
#[cfg(not(feature = "quick"))]
#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn canonical_type_payloads_follow_virtual_revisions() -> Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic()?;
    let root = fixture.path();
    std::fs::create_dir_all(root.join("src")).into_diagnostic()?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname=\"type_parity\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[workspace]\n",
    )
    .into_diagnostic()?;
    let library = root.join("src/lib.rs");
    let child = root.join("src/child.rs");
    let source = include_str!("fixtures/semantic_backend/error_types.rs.txt");
    std::fs::write(&library, source).into_diagnostic()?;
    std::fs::write(&child, CHILD).into_diagnostic()?;
    std::fs::write(root.join("src/empty.rs"), "// no types\n").into_diagnostic()?;
    let target = CrateTarget::new("type_parity", root);
    let baseline = projection::baseline(root, &target)?;
    let expected = baseline.source_error_types().into_diagnostic()?;
    let mut database = cordial::SourceDatabase::load(root).into_diagnostic()?;
    let native = database.project(&target, &[]).into_diagnostic()?;
    let actual = native.source_error_types().into_diagnostic()?;
    assert_eq!(actual, expected, "complete raw evidence and snapshots");
    assert_eq!(
        actual.report("type_parity", true).into_diagnostic()?,
        expected.report("type_parity", true).into_diagnostic()?
    );
    let rows = projection::materialized_with(&native, root, HOOKS, ATTR)?;
    assert_eq!(
        rows,
        projection::materialized_with(&baseline, root, HOOKS, ATTR)?
    );
    let report = actual.report("type_parity", true).into_diagnostic()?;
    let linked = report
        .nodes()
        .iter()
        .find(|node| node.type_path() == "Family::Io")
        .ok_or_else(|| miette::miette!("linked error variant"))?;
    assert!(linked.reaches_foreign());
    assert_eq!(linked.chain_depth(), 2);
    let paths: Vec<_> = report
        .nodes()
        .iter()
        .map(|node| node.type_path().clone())
        .collect();
    for path in [
        "Wrapper",
        "CrossFile",
        "Family::Text",
        "Family::Io",
        "dormant::Inactive",
        "dormant::inner::Nested",
        "CordialError",
        "Local",
        "Unicode",
    ] {
        assert!(paths.iter().any(|found| found == path), "missing {path}");
    }
    assert!(
        !paths
            .iter()
            .any(|path| path.contains("Hidden") || path == "Unrelated")
    );
    database
        .change_file(&child, "// implementor removed\n".into())
        .into_diagnostic()?;
    let changed = database
        .project(&target, &[])
        .into_diagnostic()?
        .source_error_types()
        .into_diagnostic()?;
    assert_ne!(changed, actual);
    assert!(
        !changed
            .report("type_parity", true)
            .into_diagnostic()?
            .nodes()
            .iter()
            .any(|node| node.type_path() == "CrossFile")
    );
    assert_eq!(std::fs::read_to_string(&child).into_diagnostic()?, CHILD);
    database
        .change_file(&child, CHILD.into())
        .into_diagnostic()?;
    assert_eq!(
        database
            .project(&target, &[])
            .into_diagnostic()?
            .source_error_types()
            .into_diagnostic()?,
        actual
    );
    database
        .change_file(&root.join("src/empty.rs"), "fn invalid( {".into())
        .into_diagnostic()?;
    assert!(database.project(&target, &[]).is_err());
    drop(database);
    std::fs::remove_dir_all(root.join("src")).into_diagnostic()?;
    assert_eq!(
        projection::materialized_with(&native, root, HOOKS, ATTR)?,
        rows
    );
    // Both shared materializers work without access to their source carrier.
    assert!(
        !projection::materialized_with(
            &native,
            root,
            &["error-site-inventory"],
            "error_site_kind"
        )?
        .is_empty()
    );
    Ok(())
}

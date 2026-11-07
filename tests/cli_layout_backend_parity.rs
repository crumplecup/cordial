//! CLI catalog parity, graph-only policy and revision recovery.
#![cfg(feature = "cli_layout")]
// Slow (over 60s): compiled only without the default `quick` feature; run with `just test-slow`.
#![cfg(not(feature = "quick"))]
use cordial::CliLayoutInventoryEnricher;
use cordial::{
    CrateIr, CrateTarget, CrateViewMut, IrView, LoadContext, Loader, QueryBuilder, SessionBuilder,
    SourceDatabase, SourceEnrichView, SourceEnricher, SourceLoadView, SourceLoader, WorkspaceIr,
};
use miette::{IntoDiagnostic, Result};
use std::path::Path;

const SOURCE: &str = include_str!("fixtures/semantic_backend/cli_layout.rs.txt");

#[tracing::instrument(level = "debug", skip(target), err(level = "warn"))]
fn baseline(root: &Path, target: &CrateTarget) -> Result<CrateIr> {
    let session = SessionBuilder::new(root).build();
    let loaded = SourceLoader
        .load(LoadContext::new(&session, target))
        .into_diagnostic()?;
    let source = loaded
        .as_any()
        .downcast_ref::<SourceLoadView>()
        .ok_or_else(|| miette::miette!("unexpected loader"))?;
    let mut graph = CrateIr::new("cli_parity");
    source.populate_ir(&mut graph).into_diagnostic()?;
    Ok(graph)
}

#[tracing::instrument(level = "debug", skip(graph), err(level = "warn"))]
fn materialize(graph: &CrateIr, root: &Path) -> Result<Vec<serde_json::Value>> {
    let empty = tempfile::tempdir().into_diagnostic()?;
    let target = CrateTarget::new("empty", empty.path());
    let session = SessionBuilder::new(root).build();
    let loaded = SourceLoader
        .load(LoadContext::new(&session, &target))
        .into_diagnostic()?;
    let source = loaded
        .as_any()
        .downcast_ref::<SourceLoadView>()
        .ok_or_else(|| miette::miette!("unexpected loader"))?;
    assert!(source.files().is_empty());
    let mut workspace = WorkspaceIr::default();
    workspace.insert_crate(graph.clone());
    let mut view = CrateViewMut::new(&mut workspace, graph.crate_name().into());
    CliLayoutInventoryEnricher
        .enrich_source(SourceEnrichView::new(&mut view, source, &session))
        .into_diagnostic()?;
    let mut rows = Vec::new();
    for node in view.nodes_matching(&QueryBuilder::new().has_attr("cli_layout_rule").build()) {
        let attrs: std::collections::BTreeMap<_, _> =
            node.weight().attrs().iter().cloned().collect();
        rows.push(serde_json::json!({"attrs":attrs, "span":node.weight().span()}));
    }
    rows.sort_by_key(serde_json::Value::to_string);
    Ok(rows)
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn cli_catalog_and_findings_survive_native_projection_and_virtual_edits() -> Result<()> {
    cordial::init_tracing();
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir_all(root.path().join("src/inline")).into_diagnostic()?;
    std::fs::create_dir_all(root.path().join("src/bin")).into_diagnostic()?;
    std::fs::create_dir_all(root.path().join("src/fixtures")).into_diagnostic()?;
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname = \"cli_parity\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[workspace]\n",
    )
    .into_diagnostic()?;
    let path = root.path().join("src/lib.rs");
    let main = root.path().join("src/main.rs");
    let external = root.path().join("src/external.rs");
    std::fs::write(&path, SOURCE).into_diagnostic()?;
    std::fs::write(&main,"#[derive(clap::Parser)] struct Island;\nstruct BinError;\nimpl std::error::Error for BinError {}\nfn main() { match Cli::parse() { cli => cli.act() } }\nfn helper() {}\n").into_diagnostic()?;
    std::fs::write(&external, "#[derive(clap::Parser)] pub struct External;\n")
        .into_diagnostic()?;
    std::fs::write(
        root.path().join("src/inline/nested.rs"),
        "#[derive(clap::Parser)] pub struct Nested;\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        root.path().join("support.rs"),
        "#[derive(clap::Parser)] pub struct Support;\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        root.path().join("function_local.rs"),
        "#[derive(clap::Parser)] pub struct LocalExternal;\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        root.path().join("src/bin/tool.rs"),
        "fn main() { Cli::parse().act(); }\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        root.path().join("src/fixtures/ignored.rs"),
        "#[derive(clap::Parser)] struct Fixture;\n",
    )
    .into_diagnostic()?;
    let target = CrateTarget::new("cli_parity", root.path());
    let legacy = baseline(root.path(), &target)?;
    let mut database = SourceDatabase::load(root.path()).into_diagnostic()?;
    let native = database.project(&target, &[]).into_diagnostic()?;
    let expected = legacy
        .source_policy_inventory()
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("missing compatibility facts"))?;
    let actual = native
        .source_policy_inventory()
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("missing native facts"))?;
    assert_eq!(
        expected.cli_layout(),
        actual.cli_layout(),
        "complete catalog, dispatch, entries and snapshots"
    );
    assert_eq!(actual.crate_root(), root.path());
    for excluded in ["Ignored", "Fixture", "LocalExternal"] {
        assert!(!actual.cli_layout().catalog().types().contains_key(excluded));
    }
    let rows = materialize(&native, root.path())?;
    assert_eq!(
        materialize(&legacy, root.path())?,
        rows,
        "complete finding attributes and spans"
    );
    for rule in ["CLI-ISLAND-001", "CLI-ACT-001", "CLI-MAIN-001"] {
        assert!(
            rows.iter().any(|r| r["attrs"]["cli_layout_rule"] == rule),
            "missing {rule}"
        );
    }
    for name in [
        "Cli",
        "Commands",
        "Conditional",
        "Repeated",
        "Bound",
        "Forwarded",
    ] {
        assert!(
            !rows
                .iter()
                .any(|r| r["attrs"]["cli_layout_rule"] == "CLI-ACT-001"
                    && r["attrs"]["context"] == format!("cli_parity::{name}")),
            "lost nested dispatch for {name}"
        );
    }
    assert!(materialize(&CrateIr::new("cli_parity"), root.path()).is_err());
    let mut corrupt = native.clone();
    let mut payload = serde_json::to_value(&actual).into_diagnostic()?;
    payload
        .as_object_mut()
        .ok_or_else(|| miette::miette!("inventory object"))?
        .remove("cli_layout");
    corrupt
        .set_attr(
            corrupt.root(),
            cordial::ATTR_SOURCE_POLICY_INVENTORY,
            payload,
        )
        .into_diagnostic()?;
    assert!(
        materialize(&corrupt, root.path()).is_err(),
        "missing CLI evidence cannot establish compliance"
    );
    database.change_file(&external,"#[derive(clap::Parser)] pub struct External;\nimpl External { pub fn act(self)->Result<(),()> { Ok(()) } }\n".into()).into_diagnostic()?;
    let revised = database.project(&target, &[]).into_diagnostic()?;
    assert_ne!(materialize(&revised, root.path())?, rows);
    database
        .change_file(
            &external,
            "#[derive(clap::Parser)] pub struct External;\n".into(),
        )
        .into_diagnostic()?;
    let restored = database.project(&target, &[]).into_diagnostic()?;
    assert_eq!(
        restored
            .source_policy_inventory()
            .into_diagnostic()?
            .map(|i| i.cli_layout().clone()),
        Some(actual.cli_layout().clone())
    );
    assert_eq!(materialize(&restored, root.path())?, rows);
    database
        .change_file(&main, "fn main( {".into())
        .into_diagnostic()?;
    assert!(
        database.project(&target, &[]).is_err(),
        "invalid entry syntax must fail closed"
    );
    Ok(())
}

//! CLI catalog parity, graph-only policy and revision recovery.
use super::shared;
use cordial::CliLayoutInventoryEnricher;
use cordial::{
    CrateIr, CrateTarget, CrateViewMut, IrView, LoadContext, Loader, QueryBuilder, SessionBuilder,
    SourceDatabase, SourceEnrichView, SourceEnricher, SourceLoadView, SourceLoader, WorkspaceIr,
};
use miette::{IntoDiagnostic, Result};
use std::path::Path;

const SOURCE: &str = include_str!("../fixtures/semantic_backend/cli_layout.rs.txt");

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

pub const PACKAGE: &str = "cli_parity";

#[tracing::instrument(level = "debug", err(level = "warn"))]
pub fn write(root: &Path) -> Result<()> {
    std::fs::create_dir_all(root.join("src/inline")).into_diagnostic()?;
    std::fs::create_dir_all(root.join("src/bin")).into_diagnostic()?;
    std::fs::create_dir_all(root.join("src/fixtures")).into_diagnostic()?;
    let path = root.join("src/lib.rs");
    let main = root.join("src/main.rs");
    let external = root.join("src/external.rs");
    std::fs::write(&path, SOURCE).into_diagnostic()?;
    std::fs::write(&main,"#[derive(clap::Parser)] struct Island;\nstruct BinError;\nimpl std::error::Error for BinError {}\nfn main() { match Cli::parse() { cli => cli.act() } }\nfn helper() {}\n").into_diagnostic()?;
    std::fs::write(&external, "#[derive(clap::Parser)] pub struct External;\n")
        .into_diagnostic()?;
    std::fs::write(
        root.join("src/inline/nested.rs"),
        "#[derive(clap::Parser)] pub struct Nested;\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        root.join("support.rs"),
        "#[derive(clap::Parser)] pub struct Support;\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        root.join("function_local.rs"),
        "#[derive(clap::Parser)] pub struct LocalExternal;\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        root.join("src/bin/tool.rs"),
        "fn main() { Cli::parse().act(); }\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        root.join("src/fixtures/ignored.rs"),
        "#[derive(clap::Parser)] struct Fixture;\n",
    )
    .into_diagnostic()?;
    Ok(())
}

#[tracing::instrument(level = "debug", skip(database), err(level = "warn"))]
pub fn check(root: &Path, database: &mut SourceDatabase) -> Result<()> {
    let main = root.join("src/main.rs");
    let external = root.join("src/external.rs");
    let target = CrateTarget::new(PACKAGE, root);
    let legacy = baseline(root, &target)?;
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
    assert_eq!(actual.crate_root(), root);
    for excluded in ["Ignored", "Fixture", "LocalExternal"] {
        assert!(!actual.cli_layout().catalog().types().contains_key(excluded));
    }
    let rows = materialize(&native, root)?;
    assert_eq!(
        materialize(&legacy, root)?,
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
    assert!(materialize(&CrateIr::new("cli_parity"), root).is_err());
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
        materialize(&corrupt, root).is_err(),
        "missing CLI evidence cannot establish compliance"
    );
    database.change_file(&external,"#[derive(clap::Parser)] pub struct External;\nimpl External { pub fn act(self)->Result<(),()> { Ok(()) } }\n".into()).into_diagnostic()?;
    let revised = database.project(&target, &[]).into_diagnostic()?;
    assert_ne!(materialize(&revised, root)?, rows);
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
    assert_eq!(materialize(&restored, root)?, rows);
    database
        .change_file(&main, "fn main( {".into())
        .into_diagnostic()?;
    assert!(
        database.project(&target, &[]).is_err(),
        "invalid entry syntax must fail closed"
    );
    shared::resync(database, root)?;
    Ok(())
}

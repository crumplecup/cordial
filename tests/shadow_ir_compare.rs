use cordial::rustdoc::{demo_shadow_crate, write_rustdoc_crate_json};
use cordial::testing::{
    ShadowStatus, build_shadow_pair_report_from_inventories,
    build_shadow_pair_report_from_workspace_ir, parse_rustdoc_json, rustdoc_load_view,
};
use cordial::{
    CrateIr, EnrichView, IrEnricher, LoadView, RustdocStructureEnricher, SessionBuilder,
    WorkspaceIr,
};
use miette::{IntoDiagnostic, WrapErr};

#[test]
fn workspace_ir_shadow_report_matches_inventory_oracle() -> miette::Result<()> {
    cordial::init_tracing();
    let temp = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let json_path = temp.path().join("demo.json");
    write_rustdoc_crate_json(&json_path, &demo_shadow_crate())
        .into_diagnostic()
        .wrap_err("write json")?;

    let inventory = parse_rustdoc_json(&json_path, "demo")
        .into_diagnostic()
        .wrap_err("parse")?;
    let load = rustdoc_load_view(inventory.clone());

    let mut workspace = WorkspaceIr::default();
    let mut ir = CrateIr::new("demo");
    load.populate_ir(&mut ir)
        .into_diagnostic()
        .wrap_err("populate")?;
    let session = SessionBuilder::new(temp.path()).build();
    RustdocStructureEnricher
        .enrich(EnrichView {
            ir: &mut ir,
            load: &load as &dyn LoadView,
            session: &session,
        })
        .into_diagnostic()
        .wrap_err("structure")?;
    workspace.insert_crate(ir);

    let ir_report = build_shadow_pair_report_from_workspace_ir(&workspace, "demo", "demo")
        .into_diagnostic()
        .wrap_err("ir report")?;
    let oracle = build_shadow_pair_report_from_inventories(&inventory, &inventory);
    assert_eq!(ir_report.covered_count, oracle.covered_count);
    assert_eq!(ir_report.rows[0].status, ShadowStatus::Covered);
    Ok(())
}

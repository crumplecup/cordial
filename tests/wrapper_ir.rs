use cordial::rustdoc::{
    build_wrapper_coverage_map, collect_elicit_complete_from_inventory,
    collect_trait_prereqs_for_inventory, collect_trenchcoat_pairs,
};
use cordial::rustdoc::{demo_trenchcoat_crate, write_rustdoc_crate_json};
use cordial::testing::{parse_rustdoc_json, rustdoc_load_view, wrapper_maps_equivalent};
use cordial::{
    CrateIr, EnrichView, IrEnricher, LoadView, RustdocStructureEnricher, SessionBuilder,
    WorkspaceIr, build_wrapper_coverage_from_hub_ir,
};
use miette::{IntoDiagnostic, WrapErr};

#[test]
fn hub_ir_wrapper_map_matches_inventory_oracle() -> miette::Result<()> {
    let temp = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let json_path = temp.path().join("demo.json");
    write_rustdoc_crate_json(&json_path, &demo_trenchcoat_crate())
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

    let ir_map = build_wrapper_coverage_from_hub_ir(&workspace, "demo");

    let pairs: Vec<(String, String)> = collect_trenchcoat_pairs(&inventory)
        .into_iter()
        .map(|pair| (pair.foreign_path, pair.wrapper_path))
        .collect();
    let complete = collect_elicit_complete_from_inventory(&inventory);
    let wrapper_prereqs = collect_trait_prereqs_for_inventory(&inventory);
    let oracle = build_wrapper_coverage_map(&pairs, &complete, &wrapper_prereqs);

    assert!(wrapper_maps_equivalent(&ir_map, &oracle));
    assert!(!ir_map.is_empty());
    Ok(())
}

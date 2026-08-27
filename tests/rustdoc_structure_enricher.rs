use cordial::rustdoc::{demo_impl_coverage_crate, write_rustdoc_crate_json};
use cordial::testing::{parse_rustdoc_json, rustdoc_load_view};
use cordial::{
    CrateIr, EnrichView, IrEnricher, IrView, LoadView, RustdocStructureEnricher, SessionBuilder,
    type_elicit_complete, type_public_methods, type_trait_impls, type_trait_prereqs,
};
use miette::{IntoDiagnostic, WrapErr};

#[test]
fn rustdoc_structure_enricher_materializes_type_attrs() -> miette::Result<()> {
    cordial::init_tracing();
    let temp = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let json_path = temp.path().join("demo.json");
    write_rustdoc_crate_json(&json_path, &demo_impl_coverage_crate())
        .into_diagnostic()
        .wrap_err("write json")?;

    let inventory = parse_rustdoc_json(&json_path, "demo")
        .into_diagnostic()
        .wrap_err("parse")?;
    let load = rustdoc_load_view(inventory);

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
        .wrap_err("enrich")?;

    let prereqs =
        type_trait_prereqs(&ir, "demo::Widget").ok_or_else(|| miette::miette!("prereqs"))?;
    assert!(prereqs.serialize);
    assert!(!prereqs.deserialize);

    assert_eq!(type_trait_impls(&ir, "demo::Widget"), vec!["Serialize"]);
    assert_eq!(type_public_methods(&ir, "demo::Widget"), vec!["draw"]);
    assert!(!type_elicit_complete(&ir, "demo::Widget"));
    Ok(())
}

#[test]
fn rustdoc_structure_enricher_sets_item_identity_at_loader() -> miette::Result<()> {
    cordial::init_tracing();
    let temp = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let json_path = temp.path().join("demo.json");
    write_rustdoc_crate_json(&json_path, &demo_impl_coverage_crate())
        .into_diagnostic()
        .wrap_err("write json")?;

    let inventory = parse_rustdoc_json(&json_path, "demo")
        .into_diagnostic()
        .wrap_err("parse")?;
    let load = rustdoc_load_view(inventory);

    let mut ir = CrateIr::new("demo");
    load.populate_ir(&mut ir)
        .into_diagnostic()
        .wrap_err("populate")?;

    let node = ir
        .node(
            ir.node_by_path("demo::Widget")
                .ok_or_else(|| miette::miette!("widget node"))?,
        )
        .ok_or_else(|| miette::miette!("node"))?;
    assert_eq!(
        node.attr("item_name").and_then(|v| v.as_str()),
        Some("Widget")
    );
    assert_eq!(node.attr("is_public").and_then(|v| v.as_bool()), Some(true));
    Ok(())
}

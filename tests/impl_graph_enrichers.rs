use cordial::rustdoc::{demo_impl_coverage_crate, demo_trenchcoat_crate, write_rustdoc_crate_json};
use cordial::testing::{parse_rustdoc_json, rustdoc_load_view};
use cordial::{
    CrateIr, EdgeKind, IrEnricher, IrView, LoadView, RustdocStructureEnricher, SessionBuilder,
    TraitImplEnricher, TrenchcoatEnricher, type_trait_impls, type_wraps_foreign,
};
use miette::{IntoDiagnostic, WrapErr};

#[test]
fn trait_impl_enricher_builds_edges_from_graph_attrs() -> miette::Result<()> {
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
        .enrich(&mut ir, &load as &dyn LoadView, &session)
        .into_diagnostic()
        .wrap_err("structure")?;
    TraitImplEnricher
        .enrich(&mut ir, &load as &dyn LoadView, &session)
        .into_diagnostic()
        .wrap_err("trait impl")?;

    assert_eq!(type_trait_impls(&ir, "demo::Widget"), vec!["Serialize"]);
    let widget = ir
        .node_by_path("demo::Widget")
        .ok_or_else(|| miette::miette!("widget"))?;
    let impls: Vec<String> = ir
        .children(widget, EdgeKind::Implements)
        .into_iter()
        .filter_map(|id| ir.node(id))
        .filter_map(|node| {
            node.attr("trait_short")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
        .collect();
    assert!(impls.iter().any(|short| short == "Serialize"));
    Ok(())
}

#[test]
fn trenchcoat_enricher_builds_wraps_edges_from_graph_attrs() -> miette::Result<()> {
    let temp = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let json_path = temp.path().join("demo.json");
    write_rustdoc_crate_json(&json_path, &demo_trenchcoat_crate())
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
        .enrich(&mut ir, &load as &dyn LoadView, &session)
        .into_diagnostic()
        .wrap_err("structure")?;
    TrenchcoatEnricher
        .enrich(&mut ir, &load as &dyn LoadView, &session)
        .into_diagnostic()
        .wrap_err("trenchcoat")?;

    assert_eq!(
        type_wraps_foreign(&ir, "demo::ForeignWrapper").as_deref(),
        Some("demo::Foreign")
    );
    let wrapper = ir
        .node_by_path("demo::ForeignWrapper")
        .ok_or_else(|| miette::miette!("wrapper"))?;
    let wraps: Vec<String> = ir
        .children(wrapper, EdgeKind::Wraps)
        .into_iter()
        .filter_map(|id| ir.node(id))
        .filter_map(|node| {
            node.attr("qualified_path")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
        .collect();
    assert!(wraps.iter().any(|path| path == "demo::Foreign"));
    Ok(())
}

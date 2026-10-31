//! Verify complete tracing inventories, including evidence retained in snapshots.

use cordial::{CrateIr, IrView, QueryBuilder, SourceTracingInventory};
use miette::{IntoDiagnostic, Result};

#[tracing::instrument(level = "debug", skip(graph), err(level = "warn"))]
pub fn inventory(graph: &CrateIr) -> Result<SourceTracingInventory> {
    let node = graph
        .nodes_matching(
            &QueryBuilder::new()
                .has_attr(cordial::ATTR_SOURCE_TRACING_INVENTORY)
                .build(),
        )
        .into_iter()
        .next()
        .ok_or_else(|| miette::miette!("missing tracing inventory"))?;
    graph
        .source_tracing_inventory(node.id())
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("missing tracing payload"))
}

#[tracing::instrument(level = "debug", skip(legacy, native), err(level = "warn"))]
pub fn assert_contract(legacy: &CrateIr, native: &CrateIr) -> Result<()> {
    let mut expected = serde_json::to_value(inventory(legacy)?).into_diagnostic()?;
    let actual = inventory(native)?;
    // HIR proves these standard error types implement Display; the old
    // file-local whitelist only knew String and Error. All other evidence must
    // match exactly, including digests, declaration ranges and classification.
    for function in expected["functions"]
        .as_array_mut()
        .ok_or_else(|| miette::miette!("functions missing"))?
    {
        if matches!(function["path"].as_str(), Some("explicit" | "generated")) {
            function["context"]["err_is_displayable"] = true.into();
        }
    }
    let expected: SourceTracingInventory = serde_json::from_value(expected).into_diagnostic()?;
    assert_eq!(expected.source_digest(), actual.source_digest());
    assert_eq!(expected.functions().len(), actual.functions().len());
    for (expected, actual) in expected.functions().iter().zip(actual.functions()) {
        assert_eq!(
            expected,
            actual,
            "complete tracing facts for {}",
            expected.path()
        );
    }
    for function in actual.functions() {
        assert_eq!(
            native.nodes_by_path(function.path()).len(),
            1,
            "native declaration {}",
            function.path()
        );
    }
    let restored =
        CrateIr::from_snapshot(native.snapshot().into_diagnostic()?).into_diagnostic()?;
    assert_eq!(actual, inventory(&restored)?);
    let mut damaged = native.clone();
    let node = damaged.nodes_matching(
        &QueryBuilder::new()
            .has_attr(cordial::ATTR_SOURCE_TRACING_INVENTORY)
            .build(),
    )[0]
    .id();
    damaged
        .set_attr(
            node,
            cordial::ATTR_SOURCE_TRACING_INVENTORY,
            serde_json::json!({"functions": []}),
        )
        .into_diagnostic()?;
    assert!(
        damaged.source_tracing_inventory(node).is_err(),
        "missing evidence must not become an empty inventory"
    );
    Ok(())
}

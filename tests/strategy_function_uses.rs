#![cfg(feature = "panics")]

use cordial::{ATTR_SOURCE_MODULE_SCOPE, EdgeKind, IrView};
use miette::IntoDiagnostic;
use std::fs;

#[path = "support/strategy_specimens.rs"]
mod specimens;
use specimens::{build, copy_specimen, discover, execute, function_facts};

#[test]
fn shared_inventory_retains_value_uses_and_nested_call_boundaries() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = copy_specimen("apply_function_uses")?;
    build(fixture.path())?;
    assert_eq!(execute(fixture.path(), &[])?.stdout, b"45\n");
    let path = fixture.path().join("src/main.rs");
    let source = fs::read_to_string(&path).into_diagnostic()?;
    let ir = discover(fixture.path())?;
    let entry = ir
        .node_by_path("entry")
        .ok_or_else(|| miette::miette!("missing entry"))?;
    let module = ir.parents(entry, EdgeKind::Contains)[0];
    let scope = ir
        .source_module_scope(module)
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("missing scope"))?;
    let inventory = scope
        .uses()
        .as_ref()
        .ok_or_else(|| miette::miette!("missing uses"))?;
    let facts = function_facts(&ir, "entry")?;
    assert_eq!(scope.source_digest(), facts.source_digest());
    let uses: Vec<_> = inventory
        .paths()
        .iter()
        .filter(|usage| usage.segments().last().is_some_and(|name| name == "leaf"))
        .collect();
    assert_eq!(uses.len(), 10, "retain every written leaf reference");
    assert_eq!(
        uses.iter()
            .filter(|usage| usage.owner_signature() == &Some(facts.signature_range().clone()))
            .count(),
        7
    );
    assert_eq!(uses.iter().filter(|usage| usage.nested_scope()).count(), 5);
    assert_eq!(
        uses.iter()
            .filter(|usage| usage.call_range().is_none())
            .count(),
        4
    );
    for usage in &uses {
        let range = usage.range();
        assert!(source[range.start()..range.end()].ends_with("leaf"));
        if let Some(call) = usage.call_range() {
            assert!(source[call.start()..call.end()].contains("leaf("));
        }
    }
    assert!(
        uses.iter()
            .any(|usage| source[usage.range().start()..usage.range().end()] == *"r#leaf")
    );
    assert!(
        uses.iter()
            .any(|usage| usage.segments() == &["crate", "leaf"] && usage.call_range().is_none())
    );
    assert_eq!(inventory.opaque_ranges().len(), 1);
    let opaque = &inventory.opaque_ranges()[0];
    assert!(source[opaque.start()..opaque.end()].starts_with("println!"));
    // Facts survive a fresh persisted IR read and do not need source access.
    fs::remove_file(path).into_diagnostic()?;
    assert_eq!(
        ir.source_module_scope(module).into_diagnostic()?,
        Some(scope)
    );
    Ok(())
}

#[test]
fn missing_inventory_is_unknown_and_corrupt_inventory_is_an_error() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = copy_specimen("apply_function_uses")?;
    let mut ir = discover(fixture.path())?;
    let entry = ir
        .node_by_path("entry")
        .ok_or_else(|| miette::miette!("missing entry"))?;
    let module = ir.parents(entry, EdgeKind::Contains)[0];
    let mut facts = serde_json::to_value(ir.source_module_scope(module).into_diagnostic()?)
        .into_diagnostic()?;
    facts
        .as_object_mut()
        .ok_or_else(|| miette::miette!("missing scope object"))?
        .remove("uses");
    ir.set_attr(module, ATTR_SOURCE_MODULE_SCOPE, facts.clone())
        .into_diagnostic()?;
    assert!(
        ir.source_module_scope(module)
            .into_diagnostic()?
            .is_some_and(|scope| scope.uses().is_none())
    );
    facts["uses"] = serde_json::json!({"paths": "broken", "opaque_ranges": []});
    ir.set_attr(module, ATTR_SOURCE_MODULE_SCOPE, facts)
        .into_diagnostic()?;
    assert!(ir.source_module_scope(module).is_err());
    Ok(())
}

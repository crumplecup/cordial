#![cfg(all(feature = "shadow", feature = "elicitation"))]

use cordial::rustdoc::{demo_shadow_crate, write_rustdoc_crate_json};
use cordial::testing::{parse_rustdoc_json, rustdoc_load_view};
use cordial::{CrateIr, discover_same_crate_shadow_pairs};
use miette::{IntoDiagnostic, WrapErr};

#[test]
fn discovers_widget_shadow_pair_without_map_file() -> miette::Result<()> {
    cordial::init_tracing();
    let dir = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let json = dir.path().join("demo.json");
    write_rustdoc_crate_json(&json, &demo_shadow_crate())
        .into_diagnostic()
        .wrap_err("write json")?;
    let inventory = parse_rustdoc_json(&json, "demo")
        .into_diagnostic()
        .wrap_err("parse")?;
    let view = rustdoc_load_view(inventory);
    let mut ir = CrateIr::new("demo");
    view.populate_ir(&mut ir)
        .into_diagnostic()
        .wrap_err("populate")?;

    let entries = discover_same_crate_shadow_pairs(&ir);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].target, "demo::Widget");
    assert_eq!(entries[0].shadow, "demo::WidgetShadow");
    Ok(())
}

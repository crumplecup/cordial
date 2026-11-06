//! Complete constructor provenance agrees across frontends, including rejected shapes.
#![cfg(feature = "internal_error_chain")]
#[path = "support/source_projection.rs"]
mod projection;
#[path = "support/error_apply_initializers.rs"]
mod specimens;
use cordial::{CrateTarget, IrView};
use miette::{IntoDiagnostic, Result};

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn native_initializers_preserve_binding_lifetime_and_cfg_proofs() -> Result<()> {
    cordial::init_tracing();
    let _subscriber = tracing::subscriber::set_default(tracing_subscriber::registry());
    let fixture = tempfile::tempdir().into_diagnostic()?;
    let root = fixture.path();
    std::fs::create_dir(root.join("src")).into_diagnostic()?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='initializer_proofs'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    let cases = specimens::cases();
    let mut library = String::new();
    for (index, source) in cases.iter().enumerate() {
        library.push_str(&format!("mod case_{index};\n"));
        std::fs::write(root.join(format!("src/case_{index}.rs")), source).into_diagnostic()?;
    }
    std::fs::write(root.join("src/lib.rs"), library).into_diagnostic()?;
    let target = CrateTarget::new("initializer_proofs", root);
    let baseline = projection::baseline(root, &target)?;
    let expected =
        serde_json::to_value(baseline.source_error_apply().into_diagnostic()?).into_diagnostic()?;
    let mut database = cordial::SourceDatabase::load(root).into_diagnostic()?;
    let native = database.project(&target, &[]).into_diagnostic()?;
    let actual =
        serde_json::to_value(native.source_error_apply().into_diagnostic()?).into_diagnostic()?;
    let actual_files = actual["files"]
        .as_array()
        .ok_or_else(|| miette::miette!("files"))?;
    let expected_files = expected["files"]
        .as_array()
        .ok_or_else(|| miette::miette!("files"))?;
    assert_eq!(actual_files.len(), expected_files.len());
    let mut differences = Vec::new();
    for (actual, expected) in actual_files.iter().zip(expected_files) {
        assert_eq!(actual["snapshot"], expected["snapshot"]);
        if actual["initializers"] != expected["initializers"] {
            differences.push(format!(
                "{}: native={} compatibility={}",
                actual["snapshot"]["path"], actual["initializers"], expected["initializers"]
            ));
        }
    }
    assert!(differences.is_empty(), "{}", differences.join("\n"));
    let populated = actual_files
        .iter()
        .filter(|file| {
            file["initializers"]["records"]
                .as_array()
                .is_some_and(|sites| !sites.is_empty())
        })
        .count();
    assert!(
        populated > 5,
        "both valid and rejected contracts must be exercised"
    );
    Ok(())
}

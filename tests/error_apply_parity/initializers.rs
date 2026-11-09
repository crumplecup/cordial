//! Complete constructor provenance agrees across frontends, including rejected shapes.
use super::projection;
#[path = "../support/error_apply_initializers.rs"]
mod specimens;
use cordial::{CrateTarget, IrView, SourceDatabase};
use miette::{IntoDiagnostic, Result};
use std::path::Path;

pub const PACKAGE: &str = "initializer_proofs";

#[tracing::instrument(level = "debug", err(level = "warn"))]
pub fn write(root: &Path) -> Result<()> {
    let cases = specimens::cases();
    let mut library = String::new();
    for (index, source) in cases.iter().enumerate() {
        library.push_str(&format!("mod case_{index};\n"));
        std::fs::write(root.join(format!("src/case_{index}.rs")), source).into_diagnostic()?;
    }
    std::fs::write(root.join("src/lib.rs"), library).into_diagnostic()
}

#[tracing::instrument(level = "debug", skip(database), err(level = "warn"))]
pub fn check(root: &Path, database: &mut SourceDatabase) -> Result<()> {
    let target = CrateTarget::new(PACKAGE, root);
    let baseline = projection::baseline(root, &target)?;
    let expected =
        serde_json::to_value(baseline.source_error_apply().into_diagnostic()?).into_diagnostic()?;
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

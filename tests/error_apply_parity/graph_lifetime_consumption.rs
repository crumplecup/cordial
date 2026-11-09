//! Graph lifetime plans require loader evidence.
use super::{apply_projection, projection};
use cordial::{ATTR_SOURCE_ERROR_APPLY, CrateTarget, IrView, SourceDatabase};
use miette::{IntoDiagnostic, Result};
use std::path::Path;

pub const PACKAGE: &str = "lifetime_consumer";

#[tracing::instrument(level = "debug", err(level = "warn"))]
pub fn write(root: &Path) -> Result<()> {
    std::fs::write(
        root.join("cordial.toml"),
        "[internal_error_chain]\nfield_strategy='borrowed'\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        root.join("src/lib.rs"),
        r#"use std::panic::Location;
#[derive(Debug, Clone)]
struct Cause;
impl std::error::Error for Cause {}
#[derive(Debug, Clone)]
struct Leaf { source: Cause, file: String }
impl Leaf {
    #[track_caller]
    fn new<'a>(source: &'a Cause) -> Self {
        Self { source: source.clone(), file: Location::caller().file().to_string() }
    }
}
impl std::error::Error for Leaf {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> { Some(&self.source) }
}
fn make<'a>(source: &'a Cause) -> Leaf { Leaf::new(source) }
"#,
    )
    .into_diagnostic()
}

#[tracing::instrument(level = "debug", skip(database), err(level = "warn"))]
pub fn check(root: &Path, database: &mut SourceDatabase) -> Result<()> {
    let target = CrateTarget::new(PACKAGE, root);
    let baseline = projection::baseline(root, &target)?;
    let native = database.project(&target, &[]).into_diagnostic()?;
    let actual = apply_projection::materialized(native.clone(), root)?;
    assert_eq!(actual, apply_projection::materialized(baseline, root)?);
    assert!(
        actual["plans"]
            .as_array()
            .is_some_and(|plans| plans.iter().any(|plan| plan["edits"].is_array())),
        "graph must introduce an error lifetime"
    );
    let facts =
        serde_json::to_value(native.source_error_apply().into_diagnostic()?).into_diagnostic()?;
    for boundary in ["declarations", "impls", "expr_path_blockers"] {
        let mut altered = facts.clone();
        for file in altered["files"]
            .as_array_mut()
            .ok_or_else(|| miette::miette!("files"))?
        {
            file["lifetimes"][boundary] = if boundary == "expr_path_blockers" {
                serde_json::json!(["Leaf"])
            } else {
                serde_json::json!([])
            };
        }
        let mut mutated = native.clone();
        mutated
            .set_attr(mutated.root(), ATTR_SOURCE_ERROR_APPLY, altered)
            .into_diagnostic()?;
        assert_ne!(
            apply_projection::materialized(mutated, root)?["plans"],
            actual["plans"],
            "IR lifetimes {boundary}"
        );
    }
    Ok(())
}

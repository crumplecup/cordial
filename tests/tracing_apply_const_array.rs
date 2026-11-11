//! The applier attaches `#[instrument]` to the function below a `const` array,
//! never to the `const` whose closing `];` only looks like an attribute tail.

use miette::{IntoDiagnostic, WrapErr};
use std::fs;

use cordial::run_tracing_instrument_apply;

const SOURCE: &str = include_str!("fixtures/tracing_apply/const_array_above_fn.rs");

#[test]
fn instrument_lands_on_the_function_below_a_const_array() -> miette::Result<()> {
    cordial::init_tracing();
    let temp = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let workspace = temp.path().join("workspace");
    let src_root = workspace.join("fixture_crate").join("src");
    fs::create_dir_all(&src_root).into_diagnostic()?;
    let lib = src_root.join("lib.rs");
    fs::write(&lib, SOURCE).into_diagnostic()?;
    fs::write(
        workspace.join("fixture_crate").join("Cargo.toml"),
        "[package]\nname = \"fixture_crate\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .into_diagnostic()?;
    fs::write(
        workspace.join("Cargo.toml"),
        "[workspace]\nmembers = [\"fixture_crate\"]\nresolver = \"2\"\n",
    )
    .into_diagnostic()?;
    let checklist = temp.path().join("tracing-instrument.checklist.md");
    fs::write(
        &checklist,
        "# Tracing instrument checklist\n\n**Open gaps:** 1\n\n## `fixture_crate`\n\n- [ ] `fixture_crate::matches_name` — `src/lib.rs:11` (private)\n",
    )
    .into_diagnostic()?;
    run_tracing_instrument_apply(&workspace, &checklist, None, false).into_diagnostic()?;
    let updated = fs::read_to_string(&lib).into_diagnostic()?;
    let attribute = updated
        .lines()
        .position(|line| line.starts_with("#[instrument"))
        .ok_or_else(|| miette::miette!("no attribute written:\n{updated}"))?;
    let below = updated.lines().nth(attribute + 1).unwrap_or_default();
    assert!(below.contains("fn matches_name"), "{updated}");
    Ok(())
}

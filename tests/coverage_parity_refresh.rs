//! Freeze Tier C coverage baselines from a live dual-run (gaps only — small files).
//!
//! ```text
//! PARITY_TIER_C=1 cargo test --features full --test coverage_parity_refresh -- --ignored --nocapture
//! ```

use miette::{IntoDiagnostic, WrapErr};
mod parity_support;

use std::fs;

use parity_support::{
    CsvTable, FRAMEWORK_GAPS_KEY_COLUMNS, assert_csv_row_sets_equal, read_cordial_csv,
    run_cordial_hub_coverage, run_elicit_doc_coverage, seed_elicit_framework_patches,
    tier_c_baseline_findings, tier_c_enabled, tier_c_workspace,
};

#[test]
#[ignore = "refresh Tier C homecoming gaps baseline; requires PARITY_TIER_C=1"]
fn refresh_homecoming_gaps_baseline() -> miette::Result<()> {
    if !tier_c_enabled() {
        return Ok(());
    }

    let workspace = tier_c_workspace("PARITY_HOMECOMING_ROOT", "/home/erik/repos/homecoming")
        .ok_or_else(|| miette::miette!("homecoming workspace"))?;
    let temp = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let elicit_store = temp.path().join("elicit-store");
    let elicit_coverage = temp.path().join("elicit-coverage");
    let cordial_store = temp.path().join("cordial-store");
    fs::create_dir_all(&elicit_store)
        .into_diagnostic()
        .wrap_err("dirs")?;
    fs::create_dir_all(&elicit_coverage)
        .into_diagnostic()
        .wrap_err("dirs")?;
    fs::create_dir_all(&cordial_store)
        .into_diagnostic()
        .wrap_err("dirs")?;

    seed_elicit_framework_patches(&elicit_coverage, cordial::WorkspaceHub::Homecoming)?;

    let output = run_elicit_doc_coverage(&workspace, &elicit_store, &elicit_coverage)?;
    assert!(output.status.success(), "elicit_doc run failed");
    run_cordial_hub_coverage(&workspace, &cordial_store)?;

    let elicit_gaps_path = elicit_coverage.join("gaps-impl.csv");
    let elicit_gaps = CsvTable::parse(
        &fs::read_to_string(&elicit_gaps_path)
            .into_diagnostic()
            .wrap_err("read elicit gaps")?,
    )?;
    let cordial_gaps = read_cordial_csv(cordial_store.as_path(), "gaps-impl.csv")?;
    assert_csv_row_sets_equal(
        &elicit_gaps,
        &cordial_gaps,
        FRAMEWORK_GAPS_KEY_COLUMNS,
        "homecoming gaps before freeze",
    );

    let dest = tier_c_baseline_findings("homecoming", "gaps-impl.csv");
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .into_diagnostic()
            .wrap_err("baseline dir")?;
    }
    fs::copy(&elicit_gaps_path, &dest)
        .into_diagnostic()
        .wrap_err("copy baseline")?;
    eprintln!("frozen baseline at {}", dest.display());
    Ok(())
}

#[test]
#[ignore = "refresh Tier C amenable gaps baseline; requires PARITY_TIER_C=1"]
fn refresh_amenable_gaps_baseline() -> miette::Result<()> {
    if !tier_c_enabled() {
        return Ok(());
    }

    let workspace = tier_c_workspace("PARITY_AMENABLE_ROOT", "/home/erik/repos/amenable")
        .ok_or_else(|| miette::miette!("amenable workspace"))?;
    let temp = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let elicit_store = temp.path().join("elicit-store");
    let elicit_coverage = temp.path().join("elicit-coverage");
    let cordial_store = temp.path().join("cordial-store");
    fs::create_dir_all(&elicit_store)
        .into_diagnostic()
        .wrap_err("dirs")?;
    fs::create_dir_all(&elicit_coverage)
        .into_diagnostic()
        .wrap_err("dirs")?;
    fs::create_dir_all(&cordial_store)
        .into_diagnostic()
        .wrap_err("dirs")?;

    seed_elicit_framework_patches(&elicit_coverage, cordial::WorkspaceHub::Amenable)?;

    let output = run_elicit_doc_coverage(&workspace, &elicit_store, &elicit_coverage)?;
    assert!(output.status.success(), "elicit_doc run failed");
    run_cordial_hub_coverage(&workspace, &cordial_store)?;

    let elicit_gaps_path = elicit_coverage.join("gaps-impl.csv");
    let elicit_gaps = CsvTable::parse(
        &fs::read_to_string(&elicit_gaps_path)
            .into_diagnostic()
            .wrap_err("read elicit gaps")?,
    )?;
    let cordial_gaps = read_cordial_csv(cordial_store.as_path(), "gaps-impl.csv")?;
    assert_csv_row_sets_equal(
        &elicit_gaps,
        &cordial_gaps,
        FRAMEWORK_GAPS_KEY_COLUMNS,
        "amenable gaps before freeze",
    );

    let dest = tier_c_baseline_findings("amenable", "gaps-impl.csv");
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .into_diagnostic()
            .wrap_err("baseline dir")?;
    }
    fs::copy(&elicit_gaps_path, &dest)
        .into_diagnostic()
        .wrap_err("copy baseline")?;
    eprintln!("frozen baseline at {}", dest.display());
    Ok(())
}

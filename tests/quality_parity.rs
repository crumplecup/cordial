//! Tier C quality parity — compare cordial vs elicit_doc on live workspaces.
//!
//! Run manually:
//! ```text
//! PARITY_TIER_C=1 cargo test --features full --test quality_parity -- --ignored --nocapture
//! ```
//!
//! Prerequisites:
//! - sibling checkouts: amenable, elicit_doc (or set env paths)
//! - for amenable antipatterns: `amenable` binary on PATH (registry dump for contract bounds)

mod parity_support;

use std::fs;
use std::path::{Path, PathBuf};

use miette::{IntoDiagnostic, WrapErr};
use parity_support::{
    ANTIPATTERNS_KEY_COLUMNS, CsvTable, antipatterns_baseline_open, antipatterns_cordial_open,
    assert_open_precision, assert_open_recall, read_cordial_csv, run_cordial_antipatterns,
    run_elicit_doc_quality, tier_c_enabled, tier_c_workspace,
};

fn read_elicit_quality_csv(quality_dir: &Path, artifact: &str) -> miette::Result<CsvTable> {
    let path = quality_dir.join(artifact);
    let content = fs::read_to_string(&path)
        .into_diagnostic()
        .wrap_err_with(|| format!("read elicit_doc output {}", path.display()))?;
    Ok(CsvTable::parse(&content)?)
}

fn dual_run_amenable_antipatterns(temp: &Path) -> miette::Result<(PathBuf, PathBuf)> {
    let workspace = tier_c_workspace("PARITY_AMENABLE_ROOT", "/home/erik/repos/amenable")
        .ok_or_else(|| {
            miette::miette!("amenable workspace not found — set PARITY_AMENABLE_ROOT")
        })?;

    let elicit_store = temp.join("elicit-store");
    let elicit_quality = temp.join("elicit-quality");
    let cordial_store = temp.join("cordial-store");
    fs::create_dir_all(&elicit_store)
        .into_diagnostic()
        .wrap_err("elicit store")?;
    fs::create_dir_all(&elicit_quality)
        .into_diagnostic()
        .wrap_err("elicit quality")?;
    fs::create_dir_all(&cordial_store)
        .into_diagnostic()
        .wrap_err("cordial store")?;

    let output =
        run_elicit_doc_quality("antipatterns", &workspace, &elicit_store, &elicit_quality)?;
    assert!(
        output.status.success(),
        "elicit_doc quality antipatterns failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    run_cordial_antipatterns(&workspace, &cordial_store)?;

    Ok((elicit_quality, cordial_store))
}

#[test]
#[ignore = "Tier C live dual-run: PARITY_TIER_C=1; amenable antipatterns vs elicit_doc; ~5–20 min"]
fn tier_c_amenable_antipatterns_csv_matches_elicit_doc() -> miette::Result<()> {
    if !tier_c_enabled() {
        return Ok(());
    }

    let temp = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let (elicit_quality, cordial_store) = dual_run_amenable_antipatterns(temp.path())?;

    let baseline = read_elicit_quality_csv(&elicit_quality, "antipatterns.csv")?;
    let actual = read_cordial_csv(cordial_store.as_path(), "antipatterns.csv")?;

    assert_open_recall(
        &baseline,
        &actual,
        antipatterns_baseline_open,
        antipatterns_cordial_open,
        ANTIPATTERNS_KEY_COLUMNS,
    );
    assert_open_precision(
        &baseline,
        &actual,
        antipatterns_baseline_open,
        antipatterns_cordial_open,
        ANTIPATTERNS_KEY_COLUMNS,
    );
    Ok(())
}

#[test]
#[ignore = "Tier C live dual-run: PARITY_TIER_C=1; amenable version-in-member vs elicit_doc"]
fn tier_c_amenable_version_in_member_csv_matches_elicit_doc() -> miette::Result<()> {
    if !tier_c_enabled() {
        return Ok(());
    }

    let temp = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let (elicit_quality, cordial_store) = dual_run_amenable_antipatterns(temp.path())?;

    let baseline = read_elicit_quality_csv(&elicit_quality, "version-in-member.csv")?;
    let actual = read_cordial_csv(cordial_store.as_path(), "version-in-member.csv")?;

    assert_open_recall(
        &baseline,
        &actual,
        antipatterns_baseline_open,
        antipatterns_cordial_open,
        ANTIPATTERNS_KEY_COLUMNS,
    );
    assert_open_precision(
        &baseline,
        &actual,
        antipatterns_baseline_open,
        antipatterns_cordial_open,
        ANTIPATTERNS_KEY_COLUMNS,
    );
    Ok(())
}

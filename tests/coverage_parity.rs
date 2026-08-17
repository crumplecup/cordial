//! Tier C coverage parity — compare cordial vs elicit_doc on live framework workspaces.
//!
//! Run manually:
//! ```text
//! PARITY_TIER_C=1 cargo test --features full --test coverage_parity -- --ignored --nocapture
//! ```
//!
//! Prerequisites:
//! - nightly toolchain with rustdoc JSON
//! - `cordial build sysroot` (populates ~/.cordial/sysroot)
//! - sibling checkouts: homecoming, amenable, elicit_doc (or set env paths)

mod parity_support;

use std::fs;
use std::path::{Path, PathBuf};

use miette::{IntoDiagnostic, WrapErr};
use parity_support::{
    AMENABLE_STD_KEY_COLUMNS, CsvTable, FRAMEWORK_GAPS_KEY_COLUMNS, HOMECOMING_STD_KEY_COLUMNS,
    assert_csv_row_sets_equal, cordial_sysroot_ready, read_cordial_csv, run_cordial_hub_coverage,
    run_elicit_doc_coverage, seed_elicit_framework_patches, tier_c_baseline_findings,
    tier_c_enabled, tier_c_workspace,
};

fn cordial_home() -> PathBuf {
    std::env::var("CORDIAL_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            std::env::var("HOME")
                .map(|home| PathBuf::from(home).join(".cordial"))
                .unwrap_or_else(|_| PathBuf::from(".cordial"))
        })
}

fn read_elicit_csv(coverage_dir: &Path, artifact: &str) -> miette::Result<CsvTable> {
    let path = coverage_dir.join(artifact);
    let content = fs::read_to_string(&path)
        .into_diagnostic()
        .wrap_err_with(|| format!("read elicit_doc output {}", path.display()))?;
    CsvTable::parse(&content)
}

fn dual_run_homecoming(temp: &Path) -> miette::Result<(PathBuf, PathBuf)> {
    let workspace = tier_c_workspace("PARITY_HOMECOMING_ROOT", "/home/erik/repos/homecoming")
        .ok_or_else(|| {
            miette::miette!("homecoming workspace not found — set PARITY_HOMECOMING_ROOT")
        })?;
    assert!(
        cordial_sysroot_ready(&cordial_home()),
        "sysroot cache missing — run `cordial build sysroot` first"
    );

    let elicit_store = temp.join("elicit-store");
    let elicit_coverage = temp.join("elicit-coverage");
    let cordial_store = temp.join("cordial-store");
    fs::create_dir_all(&elicit_store)
        .into_diagnostic()
        .wrap_err("elicit store")?;
    fs::create_dir_all(&elicit_coverage)
        .into_diagnostic()
        .wrap_err("elicit coverage")?;
    fs::create_dir_all(&cordial_store)
        .into_diagnostic()
        .wrap_err("cordial store")?;

    seed_elicit_framework_patches(&elicit_coverage, cordial::WorkspaceHub::Homecoming)?;

    let output = run_elicit_doc_coverage(&workspace, &elicit_store, &elicit_coverage)?;
    assert!(
        output.status.success(),
        "elicit_doc run failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let hub = run_cordial_hub_coverage(&workspace, &cordial_store)?;
    assert_eq!(hub, cordial::WorkspaceHub::Homecoming);

    Ok((elicit_coverage, cordial_store))
}

fn dual_run_amenable(temp: &Path) -> miette::Result<(PathBuf, PathBuf)> {
    let workspace = tier_c_workspace("PARITY_AMENABLE_ROOT", "/home/erik/repos/amenable")
        .ok_or_else(|| {
            miette::miette!("amenable workspace not found — set PARITY_AMENABLE_ROOT")
        })?;
    assert!(
        cordial_sysroot_ready(&cordial_home()),
        "sysroot cache missing — run `cordial build sysroot` first"
    );

    let elicit_store = temp.join("elicit-store");
    let elicit_coverage = temp.join("elicit-coverage");
    let cordial_store = temp.join("cordial-store");
    fs::create_dir_all(&elicit_store)
        .into_diagnostic()
        .wrap_err("elicit store")?;
    fs::create_dir_all(&elicit_coverage)
        .into_diagnostic()
        .wrap_err("elicit coverage")?;
    fs::create_dir_all(&cordial_store)
        .into_diagnostic()
        .wrap_err("cordial store")?;

    seed_elicit_framework_patches(&elicit_coverage, cordial::WorkspaceHub::Amenable)?;

    let output = run_elicit_doc_coverage(&workspace, &elicit_store, &elicit_coverage)?;
    assert!(
        output.status.success(),
        "elicit_doc run failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let hub = run_cordial_hub_coverage(&workspace, &cordial_store)?;
    assert_eq!(hub, cordial::WorkspaceHub::Amenable);

    Ok((elicit_coverage, cordial_store))
}

#[test]
fn tier_c_homecoming_gaps_match_frozen_baseline_when_present() -> miette::Result<()> {
    let baseline_path = tier_c_baseline_findings("homecoming", "gaps-impl.csv");
    if !baseline_path.is_file() {
        return Ok(());
    }
    if !tier_c_enabled() {
        return Ok(());
    }

    let temp = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let (_elicit_coverage, cordial_store) = dual_run_homecoming(temp.path())?;

    let baseline = CsvTable::parse(
        &fs::read_to_string(&baseline_path)
            .into_diagnostic()
            .wrap_err("read baseline")?,
    )?;
    let actual = read_cordial_csv(cordial_store.as_path(), "gaps-impl.csv")?;
    assert_csv_row_sets_equal(
        &baseline,
        &actual,
        FRAMEWORK_GAPS_KEY_COLUMNS,
        "homecoming gaps-impl.csv vs frozen baseline",
    );
    Ok(())
}

#[test]
#[ignore = "Tier C live dual-run: PARITY_TIER_C=1, nightly, sysroot; ~10–30 min"]
fn tier_c_homecoming_std_csv_matches_elicit_doc() -> miette::Result<()> {
    if !tier_c_enabled() {
        return Ok(());
    }

    let temp = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let (elicit_coverage, cordial_store) = dual_run_homecoming(temp.path())?;

    let baseline = read_elicit_csv(&elicit_coverage, "std.csv")?;
    let actual = read_cordial_csv(cordial_store.as_path(), "std.csv")?;
    assert_csv_row_sets_equal(
        &baseline,
        &actual,
        HOMECOMING_STD_KEY_COLUMNS,
        "homecoming std.csv",
    );

    let baseline_gaps = read_elicit_csv(&elicit_coverage, "gaps-impl.csv")?;
    let actual_gaps = read_cordial_csv(cordial_store.as_path(), "gaps-impl.csv")?;
    assert_csv_row_sets_equal(
        &baseline_gaps,
        &actual_gaps,
        FRAMEWORK_GAPS_KEY_COLUMNS,
        "homecoming gaps-impl.csv",
    );
    Ok(())
}

#[test]
fn tier_c_amenable_gaps_match_frozen_baseline_when_present() -> miette::Result<()> {
    let baseline_path = tier_c_baseline_findings("amenable", "gaps-impl.csv");
    if !baseline_path.is_file() {
        return Ok(());
    }
    if !tier_c_enabled() {
        return Ok(());
    }

    let temp = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let (_elicit_coverage, cordial_store) = dual_run_amenable(temp.path())?;

    let baseline = CsvTable::parse(
        &fs::read_to_string(&baseline_path)
            .into_diagnostic()
            .wrap_err("read baseline")?,
    )?;
    let actual = read_cordial_csv(cordial_store.as_path(), "gaps-impl.csv")?;
    assert_csv_row_sets_equal(
        &baseline,
        &actual,
        FRAMEWORK_GAPS_KEY_COLUMNS,
        "amenable gaps-impl.csv vs frozen baseline",
    );
    Ok(())
}

#[test]
#[ignore = "Tier C live dual-run: PARITY_TIER_C=1, nightly, sysroot; ~30+ min"]
fn tier_c_amenable_std_csv_matches_elicit_doc() -> miette::Result<()> {
    if !tier_c_enabled() {
        return Ok(());
    }

    let temp = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let (elicit_coverage, cordial_store) = dual_run_amenable(temp.path())?;

    let baseline = read_elicit_csv(&elicit_coverage, "std.csv")?;
    let actual = read_cordial_csv(cordial_store.as_path(), "std.csv")?;
    assert_csv_row_sets_equal(
        &baseline,
        &actual,
        AMENABLE_STD_KEY_COLUMNS,
        "amenable std.csv",
    );

    let baseline_gaps = read_elicit_csv(&elicit_coverage, "gaps-impl.csv")?;
    let actual_gaps = read_cordial_csv(cordial_store.as_path(), "gaps-impl.csv")?;
    assert_csv_row_sets_equal(
        &baseline_gaps,
        &actual_gaps,
        FRAMEWORK_GAPS_KEY_COLUMNS,
        "amenable gaps-impl.csv",
    );
    Ok(())
}

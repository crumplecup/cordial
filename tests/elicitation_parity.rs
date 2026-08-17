//! Tier A elicitation coverage parity — minimal-workspace impl + shadow vs frozen baselines.

use miette::{IntoDiagnostic, WrapErr};
mod parity_support;

use std::fs;

use parity_support::{
    IMPL_GAPS_KEY_COLUMNS, SHADOW_GAPS_KEY_COLUMNS, SHADOW_PAIR_KEY_COLUMNS,
    assert_csv_row_sets_equal, assert_open_precision, assert_open_recall,
    filter_shadow_gaps_by_target, filter_shadow_pair_by_item_path, impl_gaps_open,
    read_baseline_csv, read_cordial_csv, run_cordial_impl_coverage, run_cordial_shadow_coverage,
    shadow_gaps_open, workspace_path,
};

#[test]
fn minimal_workspace_url_impl_gaps_match_baseline() -> miette::Result<()> {
    let workspace = workspace_path("minimal-workspace");
    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    run_cordial_impl_coverage(&workspace, store.path(), Some("url"))?;

    let baseline = read_baseline_csv("minimal-workspace", "gaps-impl.csv")?;
    let actual = read_cordial_csv(store.path(), "gaps-impl.csv")?;

    assert_open_recall(
        &baseline,
        &actual,
        impl_gaps_open,
        impl_gaps_open,
        IMPL_GAPS_KEY_COLUMNS,
    );
    assert_open_precision(
        &baseline,
        &actual,
        impl_gaps_open,
        impl_gaps_open,
        IMPL_GAPS_KEY_COLUMNS,
    );
    Ok(())
}

#[test]
fn minimal_workspace_url_impl_coverage_links_proof_harness() -> miette::Result<()> {
    let workspace = workspace_path("minimal-workspace");
    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    run_cordial_impl_coverage(&workspace, store.path(), Some("url"))?;

    let csv = fs::read_to_string(store.path().join("findings/impl-coverage.csv"))
        .into_diagnostic()
        .wrap_err("read impl coverage")?;
    assert!(
        csv.contains("url::Widget") && csv.contains("Covered") && csv.contains("proof_test"),
        "expected proof harness linkage in impl-coverage.csv:\n{csv}"
    );
    Ok(())
}

#[test]
fn minimal_workspace_fixture_is_present() {
    let workspace = workspace_path("minimal-workspace");
    assert!(
        workspace.join("Cargo.toml").is_file(),
        "missing minimal-workspace fixture at {}",
        workspace.display()
    );
    assert!(
        workspace.join("crates/url/src/lib.rs").is_file(),
        "missing url crate in minimal-workspace"
    );
}

#[test]
fn minimal_workspace_url_shadow_gaps_match_baseline() -> miette::Result<()> {
    let workspace = workspace_path("minimal-workspace");
    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    run_cordial_shadow_coverage(&workspace, store.path(), Some("url"))?;

    let baseline = filter_shadow_gaps_by_target(
        &read_baseline_csv("minimal-workspace", "gaps-shadow.csv")?,
        "url",
    );
    let actual =
        filter_shadow_gaps_by_target(&read_cordial_csv(store.path(), "gaps-shadow.csv")?, "url");

    assert_open_recall(
        &baseline,
        &actual,
        shadow_gaps_open,
        shadow_gaps_open,
        SHADOW_GAPS_KEY_COLUMNS,
    );
    assert_open_precision(
        &baseline,
        &actual,
        shadow_gaps_open,
        shadow_gaps_open,
        SHADOW_GAPS_KEY_COLUMNS,
    );
    Ok(())
}

#[test]
fn minimal_workspace_url_shadow_pair_csv_matches_baseline() -> miette::Result<()> {
    let workspace = workspace_path("minimal-workspace");
    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    run_cordial_shadow_coverage(&workspace, store.path(), Some("url"))?;

    let baseline = filter_shadow_pair_by_item_path(
        &read_baseline_csv("minimal-workspace", "shadow-url.csv")?,
        "url::Widget",
    );
    let actual = filter_shadow_pair_by_item_path(
        &read_cordial_csv(store.path(), "shadow-url.csv")?,
        "url::Widget",
    );

    assert_csv_row_sets_equal(
        &baseline,
        &actual,
        SHADOW_PAIR_KEY_COLUMNS,
        "shadow-url.csv url::Widget",
    );
    Ok(())
}

#[test]
fn minimal_workspace_shadow_pair_report_is_present() -> miette::Result<()> {
    let workspace = workspace_path("minimal-workspace");
    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    run_cordial_shadow_coverage(&workspace, store.path(), Some("url"))?;

    let pair_csv = fs::read_to_string(store.path().join("findings/shadow-url.csv"))
        .into_diagnostic()
        .wrap_err("pair csv")?;
    assert!(pair_csv.contains("url::Widget"));
    assert!(pair_csv.contains("Covered"));
    Ok(())
}

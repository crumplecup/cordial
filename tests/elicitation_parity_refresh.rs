//! Refresh Tier A elicitation baselines from elicit_doc's pipeline fixture.
//!
//! ```text
//! cargo test --features impl_coverage --test elicitation_parity_refresh -- --ignored --nocapture
//! ```

use miette::{IntoDiagnostic, WrapErr};
mod parity_support;

use std::fs;

use parity_support::{
    CsvTable, IMPL_GAPS_KEY_COLUMNS, SHADOW_GAPS_KEY_COLUMNS, SHADOW_PAIR_KEY_COLUMNS,
    assert_open_precision, assert_open_recall, filter_impl_gaps_by_crate,
    filter_shadow_gaps_by_target, filter_shadow_pair_by_item_path, impl_gaps_open,
    normalize_elicit_impl_gaps, parity_root, run_cordial_impl_coverage,
    run_cordial_shadow_coverage, shadow_gaps_open, workspace_path,
};

const URL_CRATE: &str = "url";

#[test]
#[ignore = "refresh minimal-workspace gaps-impl baseline from elicit_doc pipeline fixture"]
fn refresh_minimal_workspace_gaps_impl_baseline() -> miette::Result<()> {
    let workspace = workspace_path("minimal-workspace");
    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    run_cordial_impl_coverage(&workspace, store.path(), Some(URL_CRATE))?;

    let mut fixture = elicit_doc::testing::PipelineFixture::new()
        .into_diagnostic()
        .wrap_err("elicit fixture")?;
    fixture
        .prepare(elicit_doc::testing::StageFixtureSet::Assess)
        .into_diagnostic()
        .wrap_err("prepare elicit fixture")?;
    elicit_doc::stage_report(
        fixture.ctx(),
        &elicit_doc::testing::full_impl_fixture_options(),
        &elicit_doc::testing::PipelineFixture::ui(),
    )
    .into_diagnostic()
    .wrap_err("elicit report stage")?;

    let elicit_gaps = CsvTable::parse(
        &fs::read_to_string(fixture.output_dir().join("gaps-impl.csv"))
            .into_diagnostic()
            .wrap_err("read elicit gaps")?,
    )?;
    let elicit_normalized =
        filter_impl_gaps_by_crate(&normalize_elicit_impl_gaps(&elicit_gaps), URL_CRATE);
    let cordial_gaps = filter_impl_gaps_by_crate(
        &parity_support::read_cordial_csv(store.path(), "gaps-impl.csv")?,
        URL_CRATE,
    );

    assert_open_recall(
        &elicit_normalized,
        &cordial_gaps,
        impl_gaps_open,
        impl_gaps_open,
        IMPL_GAPS_KEY_COLUMNS,
    );
    assert_open_precision(
        &elicit_normalized,
        &cordial_gaps,
        impl_gaps_open,
        impl_gaps_open,
        IMPL_GAPS_KEY_COLUMNS,
    );

    let dest = parity_root().join("baseline/minimal-workspace/findings/gaps-impl.csv");
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .into_diagnostic()
            .wrap_err("baseline dir")?;
    }
    fs::copy(store.path().join("findings/gaps-impl.csv"), &dest)
        .into_diagnostic()
        .wrap_err("copy baseline")?;
    eprintln!("wrote baseline {}", dest.display());
    Ok(())
}

#[test]
#[ignore = "refresh minimal-workspace shadow baselines from cordial shadow run"]
fn refresh_minimal_workspace_shadow_baselines() -> miette::Result<()> {
    let workspace = workspace_path("minimal-workspace");
    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    run_cordial_shadow_coverage(&workspace, store.path(), Some(URL_CRATE))?;

    let baseline_gaps = filter_shadow_gaps_by_target(
        &parity_support::read_cordial_csv(store.path(), "gaps-shadow.csv")?,
        URL_CRATE,
    );
    let baseline_pair = filter_shadow_pair_by_item_path(
        &parity_support::read_cordial_csv(store.path(), "shadow-url.csv")?,
        "url::Widget",
    );

    assert!(
        !baseline_gaps.rows.is_empty(),
        "expected shadow verification gap rows for url"
    );
    assert_eq!(
        baseline_pair.rows.len(),
        1,
        "expected one shadow-url row for Widget"
    );

    let findings_dir = parity_root().join("baseline/minimal-workspace/findings");
    fs::create_dir_all(&findings_dir)
        .into_diagnostic()
        .wrap_err("baseline dir")?;

    for artifact in ["gaps-shadow.csv", "shadow-url.csv"] {
        let dest = findings_dir.join(artifact);
        fs::copy(store.path().join("findings").join(artifact), &dest)
            .into_diagnostic()
            .wrap_err("copy baseline")?;
        eprintln!("wrote baseline {}", dest.display());
    }

    let _ = (
        SHADOW_GAPS_KEY_COLUMNS,
        SHADOW_PAIR_KEY_COLUMNS,
        shadow_gaps_open,
    );
    Ok(())
}

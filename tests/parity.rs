use miette::{IntoDiagnostic, WrapErr};
mod parity_support;

use std::fs;

use cordial::{
    ALLOWS_ETIQUETTE, DERIVES_ETIQUETTE, ERROR_CHAIN_ETIQUETTE, ERROR_SITES_ETIQUETTE,
    FOREIGN_ERROR_ATTENUATION_ETIQUETTE, FOREIGN_ERROR_TYPES_ETIQUETTE,
    INTERNAL_ERROR_CHAIN_ETIQUETTE,
};
use parity_support::{
    ALLOWS_KEY_COLUMNS, DERIVES_KEY_COLUMNS, ERROR_CHAIN_KEY_COLUMNS, ERROR_SITES_KEY_COLUMNS,
    FOREIGN_ERROR_ATTENUATION_KEY_COLUMNS, FOREIGN_ERROR_TYPES_KEY_COLUMNS,
    INTERNAL_ERROR_COMPLIANCE_KEY_COLUMNS, PANICS_KEY_COLUMNS, TRACING_KEY_COLUMNS,
    allows_baseline_open, allows_cordial_open, assert_open_precision, assert_open_recall,
    derives_baseline_open, derives_cordial_open, error_chain_baseline_open,
    error_chain_cordial_open, error_sites_baseline_open, error_sites_cordial_open,
    foreign_error_attenuation_baseline_open, foreign_error_attenuation_cordial_open,
    foreign_error_types_baseline_open, foreign_error_types_cordial_open,
    internal_error_compliance_baseline_open, internal_error_compliance_cordial_open,
    panics_baseline_open, panics_cordial_open, read_baseline_csv, read_cordial_csv, run_etiquette,
    run_quality, tracing_baseline_open, tracing_cordial_open, workspace_path,
};

#[test]
fn panics_parity_panic_sources() -> miette::Result<()> {
    let workspace = workspace_path("panic_sources");
    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    run_quality(&workspace, store.path())?;

    let baseline = read_baseline_csv("panic_sources", "panics.csv")?;
    let actual = read_cordial_csv(store.path(), "panics.csv")?;

    assert_open_recall(
        &baseline,
        &actual,
        panics_baseline_open,
        panics_cordial_open,
        PANICS_KEY_COLUMNS,
    );
    assert_open_precision(
        &baseline,
        &actual,
        panics_baseline_open,
        panics_cordial_open,
        PANICS_KEY_COLUMNS,
    );
    Ok(())
}

#[test]
fn tracing_parity_simple_fn() -> miette::Result<()> {
    let workspace = workspace_path("simple_fn");
    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    run_quality(&workspace, store.path())?;

    let baseline = read_baseline_csv("simple_fn", "tracing-instrument.csv")?;
    let actual = read_cordial_csv(store.path(), "tracing-instrument.csv")?;

    assert_open_recall(
        &baseline,
        &actual,
        tracing_baseline_open,
        tracing_cordial_open,
        TRACING_KEY_COLUMNS,
    );
    assert_open_precision(
        &baseline,
        &actual,
        tracing_baseline_open,
        tracing_cordial_open,
        TRACING_KEY_COLUMNS,
    );
    Ok(())
}

#[test]
fn tracing_parity_mixed_visibilities() -> miette::Result<()> {
    let workspace = workspace_path("mixed_visibilities");
    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    run_quality(&workspace, store.path())?;

    let baseline = read_baseline_csv("mixed_visibilities", "tracing-instrument.csv")?;
    let actual = read_cordial_csv(store.path(), "tracing-instrument.csv")?;

    assert_open_recall(
        &baseline,
        &actual,
        tracing_baseline_open,
        tracing_cordial_open,
        TRACING_KEY_COLUMNS,
    );
    assert_open_precision(
        &baseline,
        &actual,
        tracing_baseline_open,
        tracing_cordial_open,
        TRACING_KEY_COLUMNS,
    );
    Ok(())
}

#[test]
fn allows_parity_allow_attrs() -> miette::Result<()> {
    let workspace = workspace_path("allow_attrs");
    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    run_etiquette(&workspace, store.path(), &ALLOWS_ETIQUETTE)?;

    let baseline = read_baseline_csv("allow_attrs", "allows.csv")?;
    let actual = read_cordial_csv(store.path(), "allows.csv")?;

    assert_open_recall(
        &baseline,
        &actual,
        allows_baseline_open,
        allows_cordial_open,
        ALLOWS_KEY_COLUMNS,
    );
    assert_open_precision(
        &baseline,
        &actual,
        allows_baseline_open,
        allows_cordial_open,
        ALLOWS_KEY_COLUMNS,
    );
    Ok(())
}

#[test]
fn derives_parity_trivial_getter() -> miette::Result<()> {
    let workspace = workspace_path("trivial_getter");
    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    run_etiquette(&workspace, store.path(), &DERIVES_ETIQUETTE)?;

    let baseline = read_baseline_csv("trivial_getter", "derives.csv")?;
    let actual = read_cordial_csv(store.path(), "derives.csv")?;

    assert_open_recall(
        &baseline,
        &actual,
        derives_baseline_open,
        derives_cordial_open,
        DERIVES_KEY_COLUMNS,
    );
    assert_open_precision(
        &baseline,
        &actual,
        derives_baseline_open,
        derives_cordial_open,
        DERIVES_KEY_COLUMNS,
    );
    Ok(())
}

#[test]
fn error_sites_parity_error_sites() -> miette::Result<()> {
    let workspace = workspace_path("error_sites");
    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    run_etiquette(&workspace, store.path(), &ERROR_SITES_ETIQUETTE)?;

    let baseline = read_baseline_csv("error_sites", "error-sites.csv")?;
    let actual = read_cordial_csv(store.path(), "error-sites.csv")?;

    assert_open_recall(
        &baseline,
        &actual,
        error_sites_baseline_open,
        error_sites_cordial_open,
        ERROR_SITES_KEY_COLUMNS,
    );
    assert_open_precision(
        &baseline,
        &actual,
        error_sites_baseline_open,
        error_sites_cordial_open,
        ERROR_SITES_KEY_COLUMNS,
    );
    Ok(())
}

#[test]
fn error_chain_parity_preserved() -> miette::Result<()> {
    let workspace = workspace_path("error_chain");
    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    run_etiquette(&workspace, store.path(), &ERROR_CHAIN_ETIQUETTE)?;

    let baseline = read_baseline_csv("error_chain", "error-chain-preserved.csv")?;
    let actual = read_cordial_csv(store.path(), "error-chain-preserved.csv")?;

    assert_open_recall(
        &baseline,
        &actual,
        error_chain_baseline_open,
        error_chain_cordial_open,
        ERROR_CHAIN_KEY_COLUMNS,
    );
    assert_open_precision(
        &baseline,
        &actual,
        error_chain_baseline_open,
        error_chain_cordial_open,
        ERROR_CHAIN_KEY_COLUMNS,
    );
    Ok(())
}

#[test]
fn internal_error_chain_parity_compliance() -> miette::Result<()> {
    let workspace = workspace_path("internal_error_chain");
    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    run_etiquette(&workspace, store.path(), &INTERNAL_ERROR_CHAIN_ETIQUETTE)?;

    let baseline = read_baseline_csv("internal_error_chain", "internal-error-compliance.csv")?;
    let actual = read_cordial_csv(store.path(), "internal-error-compliance.csv")?;

    assert_open_recall(
        &baseline,
        &actual,
        internal_error_compliance_baseline_open,
        internal_error_compliance_cordial_open,
        INTERNAL_ERROR_COMPLIANCE_KEY_COLUMNS,
    );
    assert_open_precision(
        &baseline,
        &actual,
        internal_error_compliance_baseline_open,
        internal_error_compliance_cordial_open,
        INTERNAL_ERROR_COMPLIANCE_KEY_COLUMNS,
    );
    Ok(())
}

#[test]
fn foreign_error_types_parity_error_sites() -> miette::Result<()> {
    let workspace = workspace_path("error_sites");
    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    run_etiquette(&workspace, store.path(), &FOREIGN_ERROR_TYPES_ETIQUETTE)?;

    let baseline = read_baseline_csv("error_sites", "foreign-error-types.csv")?;
    let actual = read_cordial_csv(store.path(), "foreign-error-types.csv")?;

    assert_open_recall(
        &baseline,
        &actual,
        foreign_error_types_baseline_open,
        foreign_error_types_cordial_open,
        FOREIGN_ERROR_TYPES_KEY_COLUMNS,
    );
    assert_open_precision(
        &baseline,
        &actual,
        foreign_error_types_baseline_open,
        foreign_error_types_cordial_open,
        FOREIGN_ERROR_TYPES_KEY_COLUMNS,
    );
    Ok(())
}

#[test]
fn foreign_error_attenuation_parity_error_chain() -> miette::Result<()> {
    let workspace = workspace_path("error_chain");
    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    run_etiquette(
        &workspace,
        store.path(),
        &FOREIGN_ERROR_ATTENUATION_ETIQUETTE,
    )?;

    let baseline = read_baseline_csv("error_chain", "foreign-error-attenuation.csv")?;
    let actual = read_cordial_csv(store.path(), "foreign-error-attenuation.csv")?;

    assert_open_recall(
        &baseline,
        &actual,
        foreign_error_attenuation_baseline_open,
        foreign_error_attenuation_cordial_open,
        FOREIGN_ERROR_ATTENUATION_KEY_COLUMNS,
    );
    assert_open_precision(
        &baseline,
        &actual,
        foreign_error_attenuation_baseline_open,
        foreign_error_attenuation_cordial_open,
        FOREIGN_ERROR_ATTENUATION_KEY_COLUMNS,
    );
    Ok(())
}

#[test]
fn baseline_fixtures_are_present() {
    for workspace in [
        "panic_sources",
        "simple_fn",
        "mixed_visibilities",
        "allow_attrs",
        "trivial_getter",
        "error_sites",
        "error_chain",
        "internal_error_chain",
    ] {
        let artifacts: &[&str] = match workspace {
            "allow_attrs" => &["allows.csv"],
            "trivial_getter" => &["derives.csv"],
            "error_sites" => &["error-sites.csv", "foreign-error-types.csv"],
            "error_chain" => &["error-chain-preserved.csv", "foreign-error-attenuation.csv"],
            "internal_error_chain" => &["internal-error-compliance.csv"],
            _ => &["panics.csv", "tracing-instrument.csv"],
        };
        for artifact in artifacts {
            let path = parity_support::baseline_findings(workspace, artifact);
            assert!(
                fs::metadata(&path).is_ok(),
                "missing baseline artifact: {}",
                path.display()
            );
        }
    }
}

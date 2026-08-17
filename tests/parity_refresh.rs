//! Run with `cargo test --features quality --test parity_refresh -- --ignored --nocapture`
//! to rewrite CSV baselines under `tests/parity/baseline/`.

mod parity_support;

use std::fs;

use cordial::{
    ALLOWS_ETIQUETTE, DERIVES_ETIQUETTE, ERROR_CHAIN_ETIQUETTE, ERROR_SITES_ETIQUETTE,
    FOREIGN_ERROR_ATTENUATION_ETIQUETTE, FOREIGN_ERROR_TYPES_ETIQUETTE,
    INTERNAL_ERROR_CHAIN_ETIQUETTE, PANICS_ETIQUETTE, TRACING_ETIQUETTE,
};
use miette::{IntoDiagnostic, WrapErr};
use parity_support::{baseline_findings, run_etiquette, workspace_path};

fn write_baseline(
    workspace: &str,
    artifact: &str,
    etiquette: &'static dyn cordial::Etiquette,
) -> miette::Result<()> {
    let store = tempfile::tempdir().into_diagnostic()?;
    run_etiquette(&workspace_path(workspace), store.path(), etiquette)?;
    let src = store.path().join("findings").join(artifact);
    let dest = baseline_findings(workspace, artifact);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .into_diagnostic()
            .wrap_err("create baseline dir")?;
    }
    fs::copy(&src, &dest).into_diagnostic().wrap_err_with(|| {
        format!(
            "copy baseline {workspace}/{artifact} from {}",
            src.display()
        )
    })?;
    eprintln!("wrote {}", dest.display());
    Ok(())
}

#[test]
#[ignore = "refresh elicit_doc parity baselines manually"]
fn refresh_parity_baselines() -> miette::Result<()> {
    write_baseline("panic_sources", "panics.csv", &PANICS_ETIQUETTE)?;
    write_baseline(
        "panic_sources",
        "tracing-instrument.csv",
        &TRACING_ETIQUETTE,
    )?;
    write_baseline("simple_fn", "tracing-instrument.csv", &TRACING_ETIQUETTE)?;
    write_baseline(
        "mixed_visibilities",
        "tracing-instrument.csv",
        &TRACING_ETIQUETTE,
    )?;
    write_baseline("allow_attrs", "allows.csv", &ALLOWS_ETIQUETTE)?;
    write_baseline("trivial_getter", "derives.csv", &DERIVES_ETIQUETTE)?;
    write_baseline("error_sites", "error-sites.csv", &ERROR_SITES_ETIQUETTE)?;
    write_baseline(
        "error_chain",
        "error-chain-preserved.csv",
        &ERROR_CHAIN_ETIQUETTE,
    )?;
    write_baseline(
        "internal_error_chain",
        "internal-error-compliance.csv",
        &INTERNAL_ERROR_CHAIN_ETIQUETTE,
    )?;
    write_baseline(
        "error_sites",
        "foreign-error-types.csv",
        &FOREIGN_ERROR_TYPES_ETIQUETTE,
    )?;
    write_baseline(
        "error_chain",
        "foreign-error-attenuation.csv",
        &FOREIGN_ERROR_ATTENUATION_ETIQUETTE,
    )?;
    Ok(())
}

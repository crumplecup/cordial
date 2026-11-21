//! Upstream allocation measurements at the actual database persistence boundary.
#![cfg(feature = "hotpath-alloc")]

#[path = "support/identity_specimen.rs"]
mod identity_specimen;

use cordial::{SavedState, SemanticCargoOptions, SourceDatabase};
use identity_specimen::{caller_identities, project, specimen};
use miette::{IntoDiagnostic, Result};
use serde_json::Value;

#[test]
fn allocation_fixture() -> Result<()> {
    let Some(root) = std::env::var_os("CORDIAL_SAVED_STATE_FIXTURE") else {
        return Ok(());
    };
    let root = std::path::Path::new(&root);
    let source = specimen()?;
    let saved = SavedState::at(root.join("analysis-state"));
    let options = SemanticCargoOptions::default();
    let mut cold = SourceDatabase::load_saved(source.path(), &options, Some(saved.clone()))
        .into_diagnostic()?;
    assert!(!cold.restored());
    let mut capture = cordial::profiling::Capture::start(
        root,
        cordial::profiling::Identity {
            role: "test".to_owned(),
            boundary: "cold projection/save through warm restore/projection".to_owned(),
            ..Default::default()
        },
    )
    .into_diagnostic()?;
    let first = project(&mut cold, source.path())?;
    let expected = caller_identities(&first)?;
    drop(first);
    drop(cold);

    let mut warm =
        SourceDatabase::load_saved(source.path(), &options, Some(saved)).into_diagnostic()?;
    assert!(
        warm.restored(),
        "{:?}",
        cordial::host::take_notes().events()
    );
    let restored = project(&mut warm, source.path())?;
    assert_eq!(caller_identities(&restored)?, expected);
    drop(restored);
    drop(warm);
    capture
        .finish(cordial::profiling::FinishReason::Normal)
        .into_diagnostic()?;
    Ok(())
}

#[test]
fn upstream_hotpath_measures_database_save_and_restore() -> Result<()> {
    let root = tempfile::tempdir().into_diagnostic()?;
    let output = std::process::Command::new(std::env::current_exe().into_diagnostic()?)
        .args(["--exact", "allocation_fixture"])
        .env("CORDIAL_SAVED_STATE_FIXTURE", root.path())
        .env("HOTPATH_ALLOC_CUMULATIVE", "1")
        .env("HOTPATH_ALLOC_METRIC", "bytes")
        .env("HOTPATH_METRICS_SERVER_OFF", "1")
        .env_remove("HOTPATH_OUTPUT_PATH")
        .env_remove("HOTPATH_OUTPUT_FORMAT")
        .env_remove("CORDIAL_PERF_PROFILE_DIR")
        .output()
        .into_diagnostic()?;
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let captures = cordial::perf::read_captures(root.path()).into_diagnostic()?;
    let manifest = captures[0]
        .manifest
        .as_ref()
        .ok_or_else(|| miette::miette!("missing capture manifest"))?;
    let report: Value = serde_json::from_slice(&std::fs::read(&manifest.report).into_diagnostic()?)
        .into_diagnostic()?;
    let rows = report["functions_alloc"]["data"]
        .as_array()
        .ok_or_else(|| miette::miette!("missing allocation rows"))?;
    let salsa_bytes = std::fs::metadata(root.path().join("analysis-state/salsa"))
        .into_diagnostic()?
        .len();
    eprintln!("encoded Salsa state: {salsa_bytes} bytes");
    for (name, calls) in [
        ("SavedState::save", 1),
        ("SavedState::restore", 1),
        ("SavedState::read_part", 5),
        ("SavedState::decode_salsa", 1),
    ] {
        let row = rows
            .iter()
            .find(|row| {
                row["name"]
                    .as_str()
                    .is_some_and(|path| path.ends_with(name))
            })
            .ok_or_else(|| miette::miette!("missing {name} row"))?;
        assert_eq!(row["calls"], calls, "{name}");
        eprintln!("{name}: {}", row["total"]);
    }
    Ok(())
}

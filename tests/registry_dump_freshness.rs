//! A cached registry dump is reused only while it is newer than the
//! workspace sources and was built with the current feature set.

use std::time::{Duration, SystemTime};

use miette::IntoDiagnostic;

use cordial::testing::{AMENABLE_DUMP_REGISTRY_FEATURES, registry_dump_is_fresh};

fn set_mtime(path: &std::path::Path, time: SystemTime) -> miette::Result<()> {
    std::fs::File::options()
        .write(true)
        .open(path)
        .into_diagnostic()?
        .set_modified(time)
        .into_diagnostic()
}

#[test]
fn dump_is_fresh_only_when_newer_than_sources_and_features_match() -> miette::Result<()> {
    cordial::init_tracing();
    let dir = tempfile::tempdir().into_diagnostic()?;
    let workspace = dir.path().join("ws");
    std::fs::create_dir_all(workspace.join("src")).into_diagnostic()?;
    let source = workspace.join("src/lib.rs");
    std::fs::write(&source, "").into_diagnostic()?;
    let dump = dir.path().join("amenable-registry.dump.json");
    std::fs::write(&dump, "{}").into_diagnostic()?;
    let now = SystemTime::now();
    set_mtime(&source, now - Duration::from_secs(100))?;
    set_mtime(&dump, now)?;

    assert!(
        !registry_dump_is_fresh(&workspace, &dump),
        "no features sidecar"
    );

    std::fs::write(
        dump.with_extension("features"),
        AMENABLE_DUMP_REGISTRY_FEATURES,
    )
    .into_diagnostic()?;
    assert!(registry_dump_is_fresh(&workspace, &dump));

    set_mtime(&source, now + Duration::from_secs(100))?;
    assert!(!registry_dump_is_fresh(&workspace, &dump), "source newer");

    set_mtime(&source, now - Duration::from_secs(100))?;
    std::fs::write(dump.with_extension("features"), "creusot").into_diagnostic()?;
    assert!(
        !registry_dump_is_fresh(&workspace, &dump),
        "features differ"
    );
    Ok(())
}

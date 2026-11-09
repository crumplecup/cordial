//! Complete evidence/finding parity, configuration independence and virtual revisions.
use super::{projection, shared};
use cordial::{ATTR_SOURCE_OBSERVABILITY, CrateIr, CrateTarget, IrView, SourceDatabase};
use miette::{IntoDiagnostic, Result};
use std::path::Path;

const LIB: &str = include_str!("../fixtures/semantic_backend/observability.rs.txt");
const MAIN: &str = "fn main() -> Result<(), ()> {\n    fn nested() { try_init(); warn!(\"nested\"); }\n    println!(\"main\");\n    Ok(())\n}\n";

pub const PACKAGE: &str = "observability_parity";

#[tracing::instrument(level = "debug", err(level = "warn"))]
pub fn write(root: &Path) -> Result<()> {
    for dir in ["src/bin", "tests/fixtures"] {
        std::fs::create_dir_all(root.join(dir)).into_diagnostic()?;
    }
    let main = root.join("src/main.rs");
    std::fs::write(root.join("src/lib.rs"), LIB).into_diagnostic()?;
    std::fs::write(&main, MAIN).into_diagnostic()?;
    std::fs::write(
        root.join("src/bin/protocol.rs"),
        "fn main() { println!(\"cargo:rerun-if-changed=src\"); external::boot(); }\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        root.join("tests/integration.rs"),
        "#[test]\nfn check() { eprint!(\"integration\"); }\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        root.join("tests/fixtures/valid.rs"),
        "fn fixture() { println!(\"fixture\"); }\n",
    )
    .into_diagnostic()?;
    std::fs::write(root.join("tests/fixtures/broken.rs"), "fn invalid( {").into_diagnostic()?;
    Ok(())
}

#[tracing::instrument(level = "debug", skip(database), err(level = "warn"))]
pub fn check(root: &Path, database: &mut SourceDatabase) -> Result<()> {
    let main = root.join("src/main.rs");
    let target = CrateTarget::new(PACKAGE, root);
    let legacy = projection::baseline(root, &target)?;
    let expected = legacy.source_observability().into_diagnostic()?;
    let native = database.project(&target, &[]).into_diagnostic()?;
    assert_eq!(
        expected,
        native.source_observability().into_diagnostic()?,
        "entire payload including snapshots and raw call facts"
    );
    let rows = projection::findings(&native, root)?;
    assert_eq!(
        rows,
        projection::findings(&legacy, root)?,
        "all attributes and spans"
    );
    for rule in [
        "TRACING-SUBSCRIBER-MAIN",
        "TRACING-BOUNDARY-MAIN-SILENT",
        "TRACING-STD-PRINTLN",
        "TRACING-STD-EPRINT",
    ] {
        assert!(
            rows.iter().any(|row| row["attrs"]
                .as_object()
                .is_some_and(|attrs| attrs.values().any(|value| value == rule))),
            "missing {rule}"
        );
    }
    assert!(!rows.iter().any(|row| {
        row["attrs"]["file"]
            .as_str()
            .is_some_and(|file| file.contains("fixtures"))
    }));
    assert!(projection::findings(&CrateIr::new("empty"), root).is_err());
    let mut corrupt = native.clone();
    let mut payload = serde_json::to_value(&expected).into_diagnostic()?;
    payload["files"][0]["functions"][0]["body"]
        .as_object_mut()
        .ok_or_else(|| miette::miette!("body facts"))?
        .remove("called_paths");
    corrupt
        .set_attr(corrupt.root(), ATTR_SOURCE_OBSERVABILITY, payload)
        .into_diagnostic()?;
    assert!(
        projection::findings(&corrupt, root).is_err(),
        "required body fields fail closed"
    );
    // A policy change must use the same raw inventory, without recollection.
    std::fs::write(root.join("cordial.toml"), "[tracing.subscriber]\nknown_helper_paths=[\"external::boot\"]\n[tracing.boundary]\nmain_reports_errors=false\n[tracing.stdio]\nskip_cargo_protocol=false\nskip_folders=[\"tests/fixtures/broken.rs\"]\neprint=false\n").into_diagnostic()?;
    let configured = projection::findings(&native, root)?;
    assert_eq!(configured, projection::findings(&legacy, root)?);
    assert_ne!(configured, rows);
    assert!(configured.iter().any(|row| {
        row["attrs"]["file"]
            .as_str()
            .is_some_and(|file| file.ends_with("fixtures/valid.rs"))
    }));
    assert!(
        !configured
            .iter()
            .any(|row| row["attrs"]["boundary_rule_id"].is_string())
    );
    assert!(
        !configured
            .iter()
            .any(|row| row["attrs"]["print_rule_id"] == "TRACING-STD-EPRINT")
    );
    std::fs::write(
        root.join("cordial.toml"),
        "[tracing.stdio]\nskip_folders=[]\n",
    )
    .into_diagnostic()?;
    assert!(
        projection::findings(&native, root).is_err(),
        "including invalid fixtures must not establish compliance"
    );
    std::fs::write(root.join("cordial.toml"), "").into_diagnostic()?;
    database
        .change_file(
            &main,
            "#[instrument(err)]\nfn main() -> Result<(), ()> { forwarding_twice(); Ok(()) }\n"
                .into(),
        )
        .into_diagnostic()?;
    let revised = database.project(&target, &[]).into_diagnostic()?;
    assert_ne!(projection::findings(&revised, root)?, rows);
    assert_eq!(
        std::fs::read_to_string(&main).into_diagnostic()?,
        MAIN,
        "virtual changes must stay off disk"
    );
    database.change_file(&main, MAIN.into()).into_diagnostic()?;
    let restored = database.project(&target, &[]).into_diagnostic()?;
    assert_eq!(restored.source_observability().into_diagnostic()?, expected);
    assert_eq!(projection::findings(&restored, root)?, rows);
    database
        .change_file(&main, "fn main( {".into())
        .into_diagnostic()?;
    assert!(
        database.project(&target, &[]).is_err(),
        "invalid executable source must fail closed"
    );
    shared::resync(database, root)?;
    Ok(())
}

//! Complete evidence/finding parity, configuration independence and virtual revisions.
#![cfg(feature = "tracing")]
#[path = "support/observability_projection.rs"]
mod projection;
use cordial::{ATTR_SOURCE_OBSERVABILITY, CrateIr, CrateTarget, IrView, SourceDatabase};
use miette::{IntoDiagnostic, Result};

const LIB: &str = include_str!("fixtures/semantic_backend/observability.rs.txt");
const MAIN: &str = "fn main() -> Result<(), ()> {\n    fn nested() { try_init(); warn!(\"nested\"); }\n    println!(\"main\");\n    Ok(())\n}\n";

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn observability_evidence_and_policy_use_canonical_trees_and_revisions() -> Result<()> {
    cordial::init_tracing();
    let _subscriber = tracing::subscriber::set_default(tracing_subscriber::registry());
    let root = tempfile::tempdir().into_diagnostic()?;
    for dir in ["src/bin", "tests/fixtures"] {
        std::fs::create_dir_all(root.path().join(dir)).into_diagnostic()?;
    }
    std::fs::write(root.path().join("Cargo.toml"), "[package]\nname=\"observability_parity\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[workspace]\n").into_diagnostic()?;
    let main = root.path().join("src/main.rs");
    std::fs::write(root.path().join("src/lib.rs"), LIB).into_diagnostic()?;
    std::fs::write(&main, MAIN).into_diagnostic()?;
    std::fs::write(
        root.path().join("src/bin/protocol.rs"),
        "fn main() { println!(\"cargo:rerun-if-changed=src\"); external::boot(); }\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        root.path().join("tests/integration.rs"),
        "#[test]\nfn check() { eprint!(\"integration\"); }\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        root.path().join("tests/fixtures/valid.rs"),
        "fn fixture() { println!(\"fixture\"); }\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        root.path().join("tests/fixtures/broken.rs"),
        "fn invalid( {",
    )
    .into_diagnostic()?;
    let target = CrateTarget::new("observability_parity", root.path());
    let legacy = projection::baseline(root.path(), &target)?;
    let expected = legacy.source_observability().into_diagnostic()?;
    let mut database = SourceDatabase::load(root.path()).into_diagnostic()?;
    let native = database.project(&target, &[]).into_diagnostic()?;
    assert_eq!(
        expected,
        native.source_observability().into_diagnostic()?,
        "entire payload including snapshots and raw call facts"
    );
    let rows = projection::findings(&native, root.path())?;
    assert_eq!(
        rows,
        projection::findings(&legacy, root.path())?,
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
    assert!(projection::findings(&CrateIr::new("empty"), root.path()).is_err());
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
        projection::findings(&corrupt, root.path()).is_err(),
        "required body fields fail closed"
    );

    // A policy change must use the same raw inventory, without recollection.
    std::fs::write(root.path().join("cordial.toml"), "[tracing.subscriber]\nknown_helper_paths=[\"external::boot\"]\n[tracing.boundary]\nmain_reports_errors=false\n[tracing.stdio]\nskip_cargo_protocol=false\nskip_folders=[\"tests/fixtures/broken.rs\"]\neprint=false\n").into_diagnostic()?;
    let configured = projection::findings(&native, root.path())?;
    assert_eq!(configured, projection::findings(&legacy, root.path())?);
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
        root.path().join("cordial.toml"),
        "[tracing.stdio]\nskip_folders=[]\n",
    )
    .into_diagnostic()?;
    assert!(
        projection::findings(&native, root.path()).is_err(),
        "including invalid fixtures must not establish compliance"
    );
    std::fs::write(root.path().join("cordial.toml"), "").into_diagnostic()?;

    database
        .change_file(
            &main,
            "#[instrument(err)]\nfn main() -> Result<(), ()> { forwarding_twice(); Ok(()) }\n"
                .into(),
        )
        .into_diagnostic()?;
    let revised = database.project(&target, &[]).into_diagnostic()?;
    assert_ne!(projection::findings(&revised, root.path())?, rows);
    assert_eq!(
        std::fs::read_to_string(&main).into_diagnostic()?,
        MAIN,
        "virtual changes must stay off disk"
    );
    database.change_file(&main, MAIN.into()).into_diagnostic()?;
    let restored = database.project(&target, &[]).into_diagnostic()?;
    assert_eq!(restored.source_observability().into_diagnostic()?, expected);
    assert_eq!(projection::findings(&restored, root.path())?, rows);
    database
        .change_file(&main, "fn main( {".into())
        .into_diagnostic()?;
    assert!(
        database.project(&target, &[]).is_err(),
        "invalid executable source must fail closed"
    );
    Ok(())
}

//! Native antipattern evidence must preserve the complete policy-independent payload.
#![cfg(feature = "antipatterns")]
mod support {
    pub mod semantic_metrics;
}
use cordial::{
    CrateIr, CrateTarget, IrView, LoadContext, Loader, SessionBuilder, SourceDatabase,
    SourceLoadView, SourceLoader,
};
use miette::{IntoDiagnostic, Result};
use tracing_subscriber::prelude::*;

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn native_antipatterns_preserve_candidates_contracts_and_revisions() -> Result<()> {
    cordial::init_tracing();
    for name in ["amenable_kani", "amenable_creusot", "amenable_verus"] {
        let metrics = support::semantic_metrics::QueryMetrics::default();
        let _subscriber =
            tracing::subscriber::set_default(tracing_subscriber::registry().with(metrics.clone()));
        let fixture = tempfile::tempdir().into_diagnostic()?;
        let root = fixture.path();
        std::fs::create_dir_all(root.join("src")).into_diagnostic()?;
        std::fs::write(
            root.join("Cargo.toml"),
            format!("[package]\nname='{name}'\nversion='0.1.0'\nedition='2024'\n[workspace]\n"),
        )
        .into_diagnostic()?;
        let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/quality/antipatterns");
        let mut entries = std::fs::read_dir(fixtures)
            .into_diagnostic()?
            .collect::<std::io::Result<Vec<_>>>()
            .into_diagnostic()?;
        entries.sort_by_key(|entry| entry.path());
        let mut source = String::new();
        for entry in entries {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "rs") {
                continue;
            }
            let stem = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .ok_or_else(|| miette::miette!("fixture name"))?;
            std::fs::copy(&path, root.join("src").join(format!("{stem}.rs"))).into_diagnostic()?;
            source.push_str(&format!("mod {stem};\n"));
        }
        source.push_str("\npub fn revision(_unused: i32) -> Result<(), String> { Ok(()) }\n");
        let path = root.join("src/lib.rs");
        std::fs::write(&path, &source).into_diagnostic()?;
        std::fs::write(root.join("src/empty.rs"), "// empty\n").into_diagnostic()?;
        std::fs::create_dir_all(root.join("src/nested/tests/fixtures")).into_diagnostic()?;
        std::fs::write(
            root.join("src/nested/tests/fixtures/proof.rs"),
            include_str!("fixtures/quality/antipatterns/native_parity.rs"),
        )
        .into_diagnostic()?;
        let target = CrateTarget::new(name, root);
        let session = SessionBuilder::new(root).build();
        let loaded = SourceLoader
            .load(LoadContext::new(&session, &target))
            .into_diagnostic()?;
        let loader = loaded
            .as_any()
            .downcast_ref::<SourceLoadView>()
            .ok_or_else(|| miette::miette!("source view"))?;
        let mut baseline = CrateIr::new(name);
        loader.populate_ir(&mut baseline).into_diagnostic()?;
        let expected = baseline.source_antipatterns().into_diagnostic()?;
        let mut database = SourceDatabase::load(root).into_diagnostic()?;
        let native = database.project(&target, &[]).into_diagnostic()?;
        let actual = native.source_antipatterns().into_diagnostic()?;
        let mut actual_json = serde_json::to_value(&actual).into_diagnostic()?;
        let mut expected_json = serde_json::to_value(&expected).into_diagnostic()?;
        // Compatibility traversal follows filesystem order; evidence order is
        // immaterial, but every complete candidate payload must match.
        for payload in [&mut actual_json, &mut expected_json] {
            payload["catalog"]["syntax"]["sites"]
                .as_array_mut()
                .ok_or_else(|| miette::miette!("candidate inventory"))?
                .sort_by_key(serde_json::Value::to_string);
        }
        assert_eq!(
            actual_json["catalog"]["snapshots"], expected_json["catalog"]["snapshots"],
            "snapshots for {name}"
        );
        for (actual, expected) in actual_json["catalog"]["syntax"]["sites"]
            .as_array()
            .ok_or_else(|| miette::miette!("candidates"))?
            .iter()
            .zip(
                expected_json["catalog"]["syntax"]["sites"]
                    .as_array()
                    .ok_or_else(|| miette::miette!("candidates"))?,
            )
        {
            assert_eq!(actual, expected, "candidate for {name}");
        }
        for (actual, expected) in actual_json["catalog"]["contracts"]
            .as_array()
            .ok_or_else(|| miette::miette!("contracts"))?
            .iter()
            .zip(
                expected_json["catalog"]["contracts"]
                    .as_array()
                    .ok_or_else(|| miette::miette!("contracts"))?,
            )
        {
            assert_eq!(actual, expected, "contract for {name}");
        }
        assert_eq!(actual_json, expected_json, "complete evidence for {name}");
        let counts = metrics.snapshot();
        for query in ["antipattern_contract:payload", "rust_program:payload"] {
            // Kani assertions use canonical Rust expressions rather than the
            // Creusot/Verus contract dialect grammar.
            if (name == "amenable_kani" && query == "antipattern_contract:payload")
                || (name == "amenable_verus" && query == "rust_program:payload")
            {
                continue;
            }
            assert!(
                counts.get(query).copied().unwrap_or_default() > 0,
                "{query}"
            );
        }
        database.project(&target, &[]).into_diagnostic()?;
        assert_eq!(
            metrics.snapshot(),
            counts,
            "unchanged projection reuses contract and embedded-program grammar"
        );
        database
            .change_file(
                &path,
                source
                    .replace("_unused", "used")
                    .replace("String", "std::io::Error"),
            )
            .into_diagnostic()?;
        assert_ne!(
            database
                .project(&target, &[])
                .into_diagnostic()?
                .source_antipatterns()
                .into_diagnostic()?,
            actual
        );
        assert_eq!(std::fs::read_to_string(&path).into_diagnostic()?, source);
        database.change_file(&path, source).into_diagnostic()?;
        assert_eq!(
            database
                .project(&target, &[])
                .into_diagnostic()?
                .source_antipatterns()
                .into_diagnostic()?,
            actual
        );
        database
            .change_file(&path, "fn invalid( {".into())
            .into_diagnostic()?;
        assert!(database.project(&target, &[]).is_err());
        drop(database);
        drop(loaded);
        std::fs::remove_dir_all(root.join("src")).into_diagnostic()?;
        assert_eq!(native.source_antipatterns().into_diagnostic()?, actual);
    }
    Ok(())
}

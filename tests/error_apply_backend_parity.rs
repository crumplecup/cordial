//! Complete error/apply declaration contracts from both loader frontends.
#![cfg(feature = "internal_error_chain")]
#[path = "support/source_projection.rs"]
mod projection;
use cordial::{CrateTarget, IrView};
use miette::{IntoDiagnostic, Result};

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn native_declarations_preserve_complete_evidence_and_virtual_revisions() -> Result<()> {
    cordial::init_tracing();
    let _subscriber = tracing::subscriber::set_default(tracing_subscriber::registry());
    let fixture = tempfile::tempdir().into_diagnostic()?;
    let root = fixture.path();
    std::fs::create_dir_all(root.join("src/nested")).into_diagnostic()?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='apply_parity'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    let library = root.join("src/lib.rs");
    let source = format!(
        "mod nested; #[path = \"renamed.rs\"] mod linked; #[cfg(test)] mod test_only;\n{}",
        include_str!("fixtures/semantic_backend/error_apply.rs.txt").replace("//!", "//")
    );
    std::fs::write(&library, &source).into_diagnostic()?;
    for (path, text) in [
        ("nested/mod.rs", "mod leaf;\n"),
        (
            "nested/leaf.rs",
            "struct Nested { source: Box<super::super::Leaf> }\n",
        ),
        ("renamed.rs", "struct Linked { message: String }\n"),
        ("test_only.rs", "struct Excluded { message: String }\n"),
        ("orphan.rs", "struct Excluded { message: String }\n"),
        (
            "main.rs",
            "struct Excluded { message: String } fn main() {}\n",
        ),
    ] {
        std::fs::write(root.join("src").join(path), text).into_diagnostic()?;
    }
    let target = CrateTarget::new("apply_parity", root);
    let baseline = projection::baseline(root, &target)?;
    let expected =
        serde_json::to_value(baseline.source_error_apply().into_diagnostic()?).into_diagnostic()?;
    let mut database = cordial::SourceDatabase::load(root).into_diagnostic()?;
    let native = database.project(&target, &[]).into_diagnostic()?;
    let actual =
        serde_json::to_value(native.source_error_apply().into_diagnostic()?).into_diagnostic()?;
    compare(&actual, &expected);
    assert_eq!(actual["files"].as_array().map(Vec::len), Some(4));
    let declarations = actual["files"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|file| file["declarations"]["declarations"].as_array())
        .flatten();
    for declaration in declarations {
        if declaration["name"] == "Unicode" {
            let start = source
                .find("/* é */ message: String")
                .map(|start| start + "/* é */ message: ".len());
            let start = start.ok_or_else(|| miette::miette!("Unicode field"))?;
            assert_eq!(
                declaration["fields"]["Named"][0]["type_range"],
                serde_json::json!([start, start + "String".len()])
            );
        }
        if declaration["name"] == "EmptyDerive" {
            assert_eq!(declaration["allowed_attrs"], true);
        }
    }
    database
        .change_file(
            &library,
            source.replace("message: String", "message: &'static str"),
        )
        .into_diagnostic()?;
    let changed = database.project(&target, &[]).into_diagnostic()?;
    assert_ne!(
        serde_json::to_value(changed.source_error_apply().into_diagnostic()?).into_diagnostic()?,
        actual
    );
    assert_eq!(std::fs::read_to_string(&library).into_diagnostic()?, source);
    database
        .change_file(&library, source.clone())
        .into_diagnostic()?;
    compare(
        &serde_json::to_value(
            database
                .project(&target, &[])
                .into_diagnostic()?
                .source_error_apply()
                .into_diagnostic()?,
        )
        .into_diagnostic()?,
        &actual,
    );
    database
        .change_file(&library, source.replace("mod nested;", ""))
        .into_diagnostic()?;
    assert_eq!(
        database
            .project(&target, &[])
            .into_diagnostic()?
            .source_error_apply()
            .into_diagnostic()?
            .files()
            .len(),
        2
    );
    database
        .change_file(&library, "fn invalid( {".into())
        .into_diagnostic()?;
    assert!(database.project(&target, &[]).is_err());
    drop(database);
    std::fs::remove_dir_all(root.join("src")).into_diagnostic()?;
    compare(
        &serde_json::to_value(native.source_error_apply().into_diagnostic()?).into_diagnostic()?,
        &actual,
    );
    Ok(())
}

#[tracing::instrument(level = "debug", skip(actual, expected))]
fn compare(actual: &serde_json::Value, expected: &serde_json::Value) {
    match (actual, expected) {
        (serde_json::Value::Object(actual), serde_json::Value::Object(expected)) => {
            assert_eq!(
                actual.keys().collect::<Vec<_>>(),
                expected.keys().collect::<Vec<_>>()
            );
            for (key, expected) in expected {
                if let Some(actual) = actual.get(key) {
                    if actual.is_object() || actual.is_array() {
                        compare(actual, expected);
                    } else {
                        assert_eq!(actual, expected, "field {key}");
                    }
                }
            }
        }
        (serde_json::Value::Array(actual), serde_json::Value::Array(expected)) => {
            assert_eq!(actual.len(), expected.len());
            for (actual, expected) in actual.iter().zip(expected) {
                compare(actual, expected);
            }
        }
        _ => assert_eq!(actual, expected),
    }
}

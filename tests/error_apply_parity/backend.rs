//! Complete error/apply declaration contracts from both loader frontends.
use super::{projection, shared};
use cordial::{CrateTarget, IrView, SourceDatabase};
use miette::{IntoDiagnostic, Result};
use std::path::Path;

pub const PACKAGE: &str = "apply_parity";

/// The library root every revision in this scenario starts from and returns to.
#[tracing::instrument(level = "trace")]
fn source() -> String {
    format!(
        "mod nested; #[path = \"renamed.rs\"] mod linked; #[cfg(test)] mod test_only;\n{}",
        include_str!("../fixtures/semantic_backend/error_apply.rs.txt").replace("//!", "//")
    )
}

#[tracing::instrument(level = "debug", err(level = "warn"))]
pub fn write(root: &Path) -> Result<()> {
    std::fs::create_dir_all(root.join("src/nested")).into_diagnostic()?;
    std::fs::write(root.join("src/lib.rs"), source()).into_diagnostic()?;
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
    Ok(())
}

/// Edits the shared database, so it must run after every read-only scenario.
/// Every edit is virtual and the library root is restored before the end.
#[tracing::instrument(level = "debug", skip(database), err(level = "warn"))]
pub fn check(root: &Path, database: &mut SourceDatabase) -> Result<()> {
    let library = root.join("src/lib.rs");
    let source = source();
    let target = CrateTarget::new(PACKAGE, root);
    let baseline = projection::baseline(root, &target)?;
    let expected =
        serde_json::to_value(baseline.source_error_apply().into_diagnostic()?).into_diagnostic()?;
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
    // Leave the shared database as it was found: same text as the disk copy.
    shared::resync(database, root)?;
    // Consumers need only the persisted IR, not the database or the files.
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

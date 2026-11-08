//! Executable lifetime plans require the loader's snapshot-bound contracts.
#![cfg(feature = "internal_error_chain")]
#[path = "support/error_apply_projection.rs"]
mod apply_projection;
#[path = "support/source_projection.rs"]
mod projection;
use cordial::{ATTR_SOURCE_ERROR_APPLY, CrateTarget, IrView};
use miette::{IntoDiagnostic, Result};

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn local_lifetime_edits_follow_required_ir_evidence() -> Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic()?;
    let root = fixture.path();
    std::fs::create_dir(root.join("src")).into_diagnostic()?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='local_lifetime_consumer'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    let owned = r#"#[derive(Debug)]
struct Failure { message: String, line: u32 }
impl Failure { fn new<'a>(message: &'a str, line: u32) -> Self { Self { message: message.to_owned(), line } } }
impl std::fmt::Display for Failure { fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.message) } }
impl std::error::Error for Failure {}
fn make<'a>(message: &'a str, line: u32) -> Failure { Failure::new(message, line) }
"#;
    let borrowed = owned
        .replace("struct Failure {", "struct Failure<'a> {")
        .replace("message: String", "message: &'a str")
        .replace("impl Failure { fn new<'a>", "impl<'a> Failure<'a> { fn new")
        .replace("for Failure {", "for Failure<'a> {")
        .replace("impl std::", "impl<'a> std::")
        .replace("-> Failure {", "-> Failure<'a> {")
        .replace("message: message.to_owned()", "message");
    for (strategy, source) in [("borrowed", owned), ("owned", borrowed.as_str())] {
        std::fs::write(root.join("src/lib.rs"), source).into_diagnostic()?;
        std::fs::write(
            root.join("cordial.toml"),
            format!("[internal_error_chain]\nfield_strategy='{strategy}'\n"),
        )
        .into_diagnostic()?;
        let target = CrateTarget::new("local_lifetime_consumer", root);
        let baseline = projection::baseline(root, &target)?;
        let mut database = cordial::SourceDatabase::load(root).into_diagnostic()?;
        let native = database.project(&target, &[]).into_diagnostic()?;
        let actual = apply_projection::materialized(native.clone(), root)?;
        assert_eq!(
            actual,
            apply_projection::materialized(baseline, root)?,
            "{strategy}"
        );
        let unit = actual["units"]
            .as_array()
            .and_then(|units| units.iter().find(|unit| unit["field_name"] == "message"))
            .ok_or_else(|| miette::miette!("message unit"))?;
        assert!(unit["lifetime"]["facts"].is_object(), "{strategy}: {unit}");
        let facts = serde_json::to_value(native.source_error_apply().into_diagnostic()?)
            .into_diagnostic()?;
        for boundary in ["records", "lifetime_context", "value"] {
            let mut altered = facts.clone();
            let evidence = &mut altered["files"][0]["initializers"];
            match boundary {
                "records" => evidence[boundary] = serde_json::json!([]),
                "lifetime_context" => evidence["records"][0][boundary] = serde_json::json!(false),
                _ => {
                    evidence["records"][0]["fields"][0]["string"]["fact"][boundary] =
                        serde_json::json!([0, 0])
                }
            }
            let mut mutated = native.clone();
            mutated
                .set_attr(mutated.root(), ATTR_SOURCE_ERROR_APPLY, altered)
                .into_diagnostic()?;
            let changed = apply_projection::materialized(mutated, root)?;
            let changed_unit = changed["units"]
                .as_array()
                .and_then(|units| units.iter().find(|unit| unit["field_name"] == "message"))
                .ok_or_else(|| miette::miette!("mutated message unit"))?;
            assert_ne!(
                changed_unit["lifetime"]["facts"], unit["lifetime"]["facts"],
                "{strategy}: changing IR initializer {boundary} must change executable plans"
            );
        }
        for boundary in ["sites", "allowed_type_uses", "returns", "field_type"] {
            let mut altered = facts.clone();
            let evidence = &mut altered["files"][0]["local_lifetimes"];
            match boundary {
                "sites" => evidence["sites"] = serde_json::json!([]),
                "allowed_type_uses" => evidence["sites"][0][boundary] = serde_json::json!([]),
                "returns" => evidence["sites"][0]["plan"][boundary] = serde_json::json!([]),
                _ => evidence["sites"][0]["plan"][boundary] = serde_json::json!([0, 0]),
            }
            let mut mutated = native.clone();
            mutated
                .set_attr(mutated.root(), ATTR_SOURCE_ERROR_APPLY, altered)
                .into_diagnostic()?;
            assert_ne!(
                apply_projection::materialized(mutated, root)?["units"],
                actual["units"],
                "{strategy}: changing IR {boundary} must change executable lifetime plans"
            );
        }
    }
    Ok(())
}

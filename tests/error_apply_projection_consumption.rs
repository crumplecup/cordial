//! Production apply consumes required snapshot-bound loader declarations.
#![cfg(feature = "internal_error_chain")]
#[path = "support/error_apply_projection.rs"]
mod apply_projection;
#[path = "support/source_projection.rs"]
mod projection;
use cordial::{ATTR_SOURCE_ERROR_APPLY, CrateIr, CrateTarget, IrView};
use miette::{IntoDiagnostic, Result};

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn native_declarations_drive_apply_and_reject_missing_or_stale_evidence() -> Result<()> {
    cordial::init_tracing();
    let _subscriber = tracing::subscriber::set_default(tracing_subscriber::registry());
    let fixture = tempfile::tempdir().into_diagnostic()?;
    let root = fixture.path();
    std::fs::create_dir_all(root.join("src")).into_diagnostic()?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='apply_consumer'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        root.join("src/lib.rs"),
        r#"mod child;
const MESSAGE: &'static str = "message";
#[derive(Debug)]
struct MessageError { message: String }
impl std::error::Error for MessageError {}
impl MessageError { fn new() -> Self { Self { message: String::from(self::MESSAGE) } } }
#[derive(Debug, Clone)]
struct Cause;
impl std::error::Error for Cause {}
#[derive(Debug, Clone)]
struct Parent<'a> { source: Cause, file: String, anchor: &'a () }
impl std::error::Error for Parent<'_> {}
#[derive(Debug, derive_more::Display, derive_more::Error, derive_getters::Getters)]
pub struct Located { file: String, line: u32 }
impl Located {
    #[track_caller]
    pub fn new() -> Self {
        let loc = std::panic::Location::caller();
        Self { file: loc.file().to_string(), line: loc.line() }
    }
}
"#,
    )
    .into_diagnostic()?;
    std::fs::write(
        root.join("src/child.rs"),
        r#"use std::panic::Location;
use super::Located as ImportedLocated;
#[track_caller]
pub(super) fn make<'a>(source: &'a super::Cause, anchor: &'a ()) -> super::Parent<'a> {
    super::Parent { source: source.clone(), file: Location::caller().file().to_string(), anchor }
}
fn read(value: &ImportedLocated) -> &String { value.file() }
"#,
    )
    .into_diagnostic()?;
    let target = CrateTarget::new("apply_consumer", root);
    let baseline = projection::baseline(root, &target)?;
    let mut database = cordial::SourceDatabase::load(root).into_diagnostic()?;
    let native = database.project(&target, &[]).into_diagnostic()?;
    for strategy in ["borrowed", "owned"] {
        std::fs::write(
            root.join("cordial.toml"),
            format!("[internal_error_chain]\nfield_strategy='{strategy}'\n"),
        )
        .into_diagnostic()?;
        let actual = apply_projection::materialized(native.clone(), root)?;
        assert_eq!(
            actual,
            apply_projection::materialized(baseline.clone(), root)?,
            "{strategy}"
        );
        if strategy == "borrowed" {
            assert!(
                actual["plans"]
                    .as_array()
                    .is_some_and(|plans| plans.iter().any(|plan| plan["edits"].is_array())),
                "cross-file graph must have executable edits"
            );
            let facts = serde_json::to_value(native.source_error_apply().into_diagnostic()?)
                .into_diagnostic()?;
            for boundary in [
                "name_audit",
                "static_strings",
                "namespace",
                "declarations",
                "uses",
                "expressions",
            ] {
                let mut incomplete = facts.clone();
                incomplete["files"][0]
                    .as_object_mut()
                    .ok_or_else(|| miette::miette!("file facts"))?
                    .remove(boundary);
                let mut mutated = native.clone();
                mutated
                    .set_attr(mutated.root(), ATTR_SOURCE_ERROR_APPLY, incomplete)
                    .into_diagnostic()?;
                assert!(
                    apply_projection::materialized(mutated, root).is_err(),
                    "missing {boundary}"
                );
            }
            for boundary in ["name_audit", "static_strings"] {
                let mut altered = facts.clone();
                for file in altered["files"]
                    .as_array_mut()
                    .ok_or_else(|| miette::miette!("files"))?
                {
                    if boundary == "name_audit" {
                        file[boundary]["ambiguous"] = serde_json::json!(true);
                    } else {
                        file[boundary] = serde_json::json!([]);
                    }
                }
                let mut mutated = native.clone();
                mutated
                    .set_attr(mutated.root(), ATTR_SOURCE_ERROR_APPLY, altered)
                    .into_diagnostic()?;
                let section = if boundary == "name_audit" {
                    "plans"
                } else {
                    "units"
                };
                assert_ne!(
                    apply_projection::materialized(mutated, root)?[section],
                    actual[section],
                    "IR {boundary}"
                );
            }
            for guard in ["has_cfg", "macro_names", "paths"] {
                let mut altered = facts.clone();
                for file in altered["files"]
                    .as_array_mut()
                    .ok_or_else(|| miette::miette!("files"))?
                {
                    file["uses"][guard] = match guard {
                        "has_cfg" => serde_json::json!(true),
                        "macro_names" => serde_json::json!(["Cause", "Parent", "Located"]),
                        _ => serde_json::json!([]),
                    };
                }
                let mut mutated = native.clone();
                mutated
                    .set_attr(mutated.root(), ATTR_SOURCE_ERROR_APPLY, altered)
                    .into_diagnostic()?;
                assert_ne!(
                    apply_projection::materialized(mutated, root)?["plans"],
                    actual["plans"],
                    "IR uses {guard}"
                );
            }
            let mut no_expressions = facts.clone();
            for file in no_expressions["files"]
                .as_array_mut()
                .ok_or_else(|| miette::miette!("files"))?
            {
                file["expressions"]["events"] = serde_json::json!([]);
            }
            let mut mutated = native.clone();
            mutated
                .set_attr(mutated.root(), ATTR_SOURCE_ERROR_APPLY, no_expressions)
                .into_diagnostic()?;
            assert_ne!(
                apply_projection::materialized(mutated, root)?["plans"],
                actual["plans"],
                "IR expression evidence"
            );
            for boundary in ["imports", "modules"] {
                let mut altered = facts.clone();
                for file in altered["files"]
                    .as_array_mut()
                    .ok_or_else(|| miette::miette!("files"))?
                {
                    file["namespace"][boundary] = serde_json::json!([]);
                }
                let mut mutated = native.clone();
                mutated
                    .set_attr(mutated.root(), ATTR_SOURCE_ERROR_APPLY, altered)
                    .into_diagnostic()?;
                let materialized = apply_projection::materialized(mutated, root)?;
                let section = if boundary == "modules" {
                    "plans"
                } else {
                    "index"
                };
                assert_ne!(materialized[section], actual[section], "IR {boundary}");
            }
            let mut aliases = facts.clone();
            for file in aliases["files"]
                .as_array_mut()
                .ok_or_else(|| miette::miette!("files"))?
            {
                file["namespace"]["aliases"] = serde_json::json!(["Located"]);
            }
            let mut ambiguous_alias = native.clone();
            ambiguous_alias
                .set_attr(ambiguous_alias.root(), ATTR_SOURCE_ERROR_APPLY, aliases)
                .into_diagnostic()?;
            assert_ne!(
                apply_projection::materialized(ambiguous_alias, root)?,
                actual,
                "getter identities must respect aliases in the IR"
            );
            let mut altered = facts.clone();
            for file in altered["files"]
                .as_array_mut()
                .ok_or_else(|| miette::miette!("files"))?
            {
                file["declarations"]["declarations"] = serde_json::json!([]);
            }
            let mut mutated = native.clone();
            mutated
                .set_attr(mutated.root(), ATTR_SOURCE_ERROR_APPLY, altered)
                .into_diagnostic()?;
            assert_ne!(
                apply_projection::materialized(mutated, root)?["plans"],
                actual["plans"],
                "planner must consume the IR declarations"
            );
            let mut stale = facts.clone();
            stale["files"][0]["snapshot"]["digest"] = serde_json::json!(vec![0u8; 32]);
            let mut stale_graph = native.clone();
            stale_graph
                .set_attr(stale_graph.root(), ATTR_SOURCE_ERROR_APPLY, stale)
                .into_diagnostic()?;
            assert!(apply_projection::materialized(stale_graph, root).is_err());
            let mut duplicate = facts.clone();
            let first = duplicate["files"][0].clone();
            duplicate["files"]
                .as_array_mut()
                .ok_or_else(|| miette::miette!("files"))?
                .push(first);
            let mut ambiguous = native.clone();
            ambiguous
                .set_attr(ambiguous.root(), ATTR_SOURCE_ERROR_APPLY, duplicate)
                .into_diagnostic()?;
            assert!(apply_projection::materialized(ambiguous, root).is_err());
        }
    }
    let missing = CrateIr::new("apply_consumer");
    assert!(missing.source_error_apply().is_err());
    assert!(apply_projection::materialized(missing, root).is_err());
    let mut malformed = native;
    malformed
        .set_attr(
            malformed.root(),
            ATTR_SOURCE_ERROR_APPLY,
            serde_json::json!({"files":false}),
        )
        .into_diagnostic()?;
    assert!(malformed.source_error_apply().is_err());
    assert!(apply_projection::materialized(malformed, root).is_err());
    Ok(())
}

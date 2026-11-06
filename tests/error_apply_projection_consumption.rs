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
#[derive(Debug, derive_more::Display, derive_more::Error, derive_getters::Getters)]
pub struct Fresh { file: String, line: u32 }
impl Fresh {
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
                "lifetimes",
                "getters",
                "getter_uses",
                "local_lifetimes",
                "initializers",
                "conversion_sites",
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
            assert!(actual.to_string().contains("CordialFreshFileStorage"));
            let mut altered = facts.clone();
            for file in altered["files"]
                .as_array_mut()
                .ok_or_else(|| miette::miette!("files"))?
            {
                file["initializers"]["records"] = serde_json::json!([]);
            }
            let mut mutated = native.clone();
            mutated
                .set_attr(mutated.root(), ATTR_SOURCE_ERROR_APPLY, altered)
                .into_diagnostic()?;
            assert_ne!(
                apply_projection::materialized(mutated, root)?["units"],
                actual["units"],
                "initializer evidence must come from the IR"
            );

            for boundary in ["fields", "names"] {
                let mut altered = facts.clone();
                for file in altered["files"]
                    .as_array_mut()
                    .ok_or_else(|| miette::miette!("files"))?
                {
                    if boundary == "fields" {
                        file["getters"]["fields"] = serde_json::json!([]);
                    } else {
                        file["getters"]["names"]["counts"]["CordialFreshFileStorage"] =
                            serde_json::json!(1);
                    }
                }
                let mut mutated = native.clone();
                mutated
                    .set_attr(mutated.root(), ATTR_SOURCE_ERROR_APPLY, altered)
                    .into_diagnostic()?;
                assert_ne!(
                    apply_projection::materialized(mutated, root)?,
                    actual,
                    "IR getter {boundary}"
                );
            }
            for boundary in ["uses", "methods"] {
                let mut altered = facts.clone();
                for file in altered["files"]
                    .as_array_mut()
                    .ok_or_else(|| miette::miette!("files"))?
                {
                    file["getter_uses"][boundary] = serde_json::json!([]);
                }
                let mut mutated = native.clone();
                mutated
                    .set_attr(mutated.root(), ATTR_SOURCE_ERROR_APPLY, altered)
                    .into_diagnostic()?;
                assert_ne!(
                    apply_projection::materialized(mutated, root)?,
                    actual,
                    "IR getter consumers {boundary}"
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

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn graph_lifetime_plans_require_loader_evidence() -> Result<()> {
    cordial::init_tracing();
    let _subscriber = tracing::subscriber::set_default(tracing_subscriber::registry());
    let fixture = tempfile::tempdir().into_diagnostic()?;
    let root = fixture.path();
    std::fs::create_dir(root.join("src")).into_diagnostic()?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='lifetime_consumer'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        root.join("cordial.toml"),
        "[internal_error_chain]\nfield_strategy='borrowed'\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        root.join("src/lib.rs"),
        r#"use std::panic::Location;
#[derive(Debug, Clone)]
struct Cause;
impl std::error::Error for Cause {}
#[derive(Debug, Clone)]
struct Leaf { source: Cause, file: String }
impl Leaf {
    #[track_caller]
    fn new<'a>(source: &'a Cause) -> Self {
        Self { source: source.clone(), file: Location::caller().file().to_string() }
    }
}
impl std::error::Error for Leaf {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> { Some(&self.source) }
}
fn make<'a>(source: &'a Cause) -> Leaf { Leaf::new(source) }
"#,
    )
    .into_diagnostic()?;
    let target = CrateTarget::new("lifetime_consumer", root);
    let baseline = projection::baseline(root, &target)?;
    let mut database = cordial::SourceDatabase::load(root).into_diagnostic()?;
    let native = database.project(&target, &[]).into_diagnostic()?;
    let actual = apply_projection::materialized(native.clone(), root)?;
    assert_eq!(actual, apply_projection::materialized(baseline, root)?);
    assert!(
        actual["plans"]
            .as_array()
            .is_some_and(|plans| plans.iter().any(|plan| plan["edits"].is_array())),
        "graph must introduce an error lifetime"
    );
    let facts =
        serde_json::to_value(native.source_error_apply().into_diagnostic()?).into_diagnostic()?;
    for boundary in ["declarations", "impls", "expr_path_blockers"] {
        let mut altered = facts.clone();
        for file in altered["files"]
            .as_array_mut()
            .ok_or_else(|| miette::miette!("files"))?
        {
            file["lifetimes"][boundary] = if boundary == "expr_path_blockers" {
                serde_json::json!(["Leaf"])
            } else {
                serde_json::json!([])
            };
        }
        let mut mutated = native.clone();
        mutated
            .set_attr(mutated.root(), ATTR_SOURCE_ERROR_APPLY, altered)
            .into_diagnostic()?;
        assert_ne!(
            apply_projection::materialized(mutated, root)?["plans"],
            actual["plans"],
            "IR lifetimes {boundary}"
        );
    }
    Ok(())
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn apply_text_uses_frozen_loader_bytes_and_rejects_a_fresh_mismatch() -> Result<()> {
    use cordial::{LoadContext, Loader, SessionBuilder, SourceLoadView, SourceLoader};
    cordial::init_tracing();
    let _subscriber = tracing::subscriber::set_default(tracing_subscriber::registry());
    let fixture = tempfile::tempdir().into_diagnostic()?;
    let root = fixture.path();
    std::fs::create_dir(root.join("src")).into_diagnostic()?;
    let path = root.join("src/lib.rs");
    std::fs::write(
        &path,
        r#"#[derive(Debug)]
struct Trouble { file: String }
impl std::error::Error for Trouble {}
impl Trouble { fn new() -> Self { Self { file: file!().to_owned() } } }
"#,
    )
    .into_diagnostic()?;
    let target = CrateTarget::new("frozen_apply", root);
    let graph = projection::baseline(root, &target)?;
    let session = SessionBuilder::new(root).build();
    let loaded = SourceLoader
        .load(LoadContext::new(&session, &target))
        .into_diagnostic()?;
    let source = loaded
        .as_any()
        .downcast_ref::<SourceLoadView>()
        .ok_or_else(|| miette::miette!("source view"))?;
    let expected = apply_projection::materialized_with_source(graph.clone(), source, &session)?;
    assert!(
        expected["units"]
            .as_array()
            .is_some_and(|units| !units.is_empty())
    );
    // An unrelated file, even malformed Rust, must not become another planner
    // input. The IR defines the complete file universe for this snapshot.
    std::fs::write(root.join("src/later.rs"), "fn malformed(").into_diagnostic()?;
    std::fs::write(&path, "fn changed() {}").into_diagnostic()?;
    assert_eq!(
        expected,
        apply_projection::materialized_with_source(graph.clone(), source, &session)?
    );
    let fresh = SourceLoader
        .load(LoadContext::new(&session, &target))
        .into_diagnostic()?;
    let source = fresh
        .as_any()
        .downcast_ref::<SourceLoadView>()
        .ok_or_else(|| miette::miette!("fresh source view"))?;
    assert!(apply_projection::materialized_with_source(graph, source, &session).is_err());
    Ok(())
}

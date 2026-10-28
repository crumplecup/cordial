//! Build the normal source IR for apply and read its classified recipes.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::enricher::ScopeEnricher;
use crate::error::{CordialError, CordialResult};
use crate::hooks::{EnrichView, IrEnricher};
use crate::ir::{BasicQuery, CrateIr, CrateView, CrateViewMut, IrView, WorkspaceIr};
use crate::loader::{CrateTarget, SourceLoadView};
use crate::session::SessionBuilder;

use super::super::enricher::FunctionInventoryEnricher;
use super::super::types::{InstrumentLevel, InstrumentRecipe};
use super::InstrumentGap;
use super::instrument::{ApplyRecipeRecord, file_matches};

/// Run the same source loader and function enricher used by quality. The
/// caller holds a parsed-source session across this build and the edit pass,
/// so reading a file to edit reuses its exact source snapshot and syntax tree.
#[tracing::instrument(level = "debug", skip(targets, by_file), err(level = "warn"))]
pub(super) fn build_apply_ir(
    project_root: &Path,
    targets: &[CrateTarget],
    by_file: &BTreeMap<(String, PathBuf), Vec<InstrumentGap>>,
) -> CordialResult<WorkspaceIr> {
    let session = SessionBuilder::new(project_root)
        .with_store_home(project_root)
        .build();
    let scope = ScopeEnricher;
    let inventory = FunctionInventoryEnricher;
    let enrichers: [&dyn IrEnricher; 2] = [&scope, &inventory];
    let mut workspace = WorkspaceIr::default();

    for target in targets {
        if !by_file.keys().any(|(name, _)| name == target.crate_name()) {
            continue;
        }
        let source = SourceLoadView::for_target(target)?;
        let mut crate_ir = CrateIr::new(target.crate_name());
        source.populate_ir(&mut crate_ir)?;
        workspace.insert_crate(crate_ir);
        crate::session::prepare_workspace_source_facts(
            &mut workspace,
            &source,
            &session,
            &enrichers,
        )?;
        for enricher in enrichers {
            let mut view = CrateViewMut::new(&mut workspace, target.crate_name().clone());
            enricher.enrich(EnrichView::new(&mut view, &source, &session))?;
        }
    }
    Ok(workspace)
}

/// Recover only the recipes for one checklist file from the enriched IR.
/// Required fields fail closed: a partial function node is a broken
/// collection contract, not a default recipe to write into source.
#[tracing::instrument(level = "debug", skip(workspace), err(level = "warn"))]
pub(super) fn recipes_for_file(
    workspace: &WorkspaceIr,
    crate_name: &str,
    rel_path: &Path,
) -> CordialResult<Vec<ApplyRecipeRecord>> {
    let view = CrateView::new(workspace, crate_name.to_string());
    let rel = rel_path.to_string_lossy().replace('\\', "/");
    let mut records = Vec::new();
    for node in view.nodes_matching(&BasicQuery::all_nodes()) {
        if node.attr("recipe_level").is_none() {
            continue;
        }
        let file = required_text(&node, "file")?;
        if !file_matches(file, &rel) {
            continue;
        }
        let recipe_level = level(required_text(&node, "recipe_level")?)?;
        let err_text = required_text(&node, "recipe_err")?;
        let err = if err_text.is_empty() {
            None
        } else {
            Some(level(err_text)?)
        };
        let ret = node
            .attr("recipe_ret")
            .and_then(|value| value.as_bool())
            .ok_or_else(missing_recipe_fact)?;
        let line = node
            .attr("line")
            .and_then(|value| value.as_u64())
            .and_then(|value| u32::try_from(value).ok())
            .ok_or_else(missing_recipe_fact)?;
        let recipe = InstrumentRecipe::builder()
            .level(recipe_level)
            .skip(csv_list(required_text(&node, "recipe_skip")?))
            .fields(csv_list(required_text(&node, "recipe_fields")?))
            .err(err)
            .ret(ret)
            .build()?;
        records.push(ApplyRecipeRecord::new(
            required_text(&node, "qualified_path")?.to_string(),
            file.to_string(),
            line,
            recipe,
        ));
    }
    Ok(records)
}

#[tracing::instrument(level = "trace", skip(node), err(level = "warn"))]
fn required_text<'a>(node: &'a crate::ir::NodeRef<'_>, key: &str) -> CordialResult<&'a str> {
    node.attr(key)
        .and_then(|value| value.as_str())
        .ok_or_else(missing_recipe_fact)
}

#[tracing::instrument(level = "trace", err(level = "warn"))]
fn level(value: &str) -> CordialResult<InstrumentLevel> {
    InstrumentLevel::from_attr(value).ok_or_else(missing_recipe_fact)
}

#[tracing::instrument(level = "trace")]
fn missing_recipe_fact() -> CordialError {
    CordialError::workspace_source_facts_missing("tracing recipe")
}

#[tracing::instrument(level = "trace")]
fn csv_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(ToString::to_string)
        .collect()
}

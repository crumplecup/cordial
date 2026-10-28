//! Build and validate edits for a discovered cross-file error component.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use tracing::instrument;

use crate::config::ErrorFieldStrategy;
use crate::loader::SourceEditView;

use super::super::super::Edit;
use super::super::VariantChange;
use super::super::declarations::DeclarationKind;
use super::super::expressions::GraphExprPlanner;
use super::super::lifetimes;
use super::super::plan::plan_struct_fields;
use super::{CrateTypeIndex, Declarations, ErrorEdgeTarget, ErrorGraphPlan, GraphDeclaration};

#[instrument(level = "debug", skip(declarations, files, strategy, index))]
pub(super) fn plan(
    declarations: &Declarations<'_>,
    files: &[SourceEditView],
    strategy: ErrorFieldStrategy,
    index: &CrateTypeIndex,
) -> Option<Vec<ErrorGraphPlan>> {
    let expected: BTreeMap<String, ErrorEdgeTarget> = declarations
        .iter()
        .map(|(name, (_, _, target))| (name.clone(), target.clone()))
        .collect();
    let names: BTreeSet<_> = declarations.keys().cloned().collect();
    let owners = declarations
        .iter()
        .map(|(name, (file, _, _))| (name.clone(), *file))
        .collect();
    let lifetime = lifetimes::select_across(strategy, &names, &owners, index);
    let changing_generics = lifetime
        .as_ref()
        .map(|plan| plan.names().clone())
        .unwrap_or_default();
    let graph_lifetime = lifetime.as_ref().map(|plan| plan.lifetime().as_str());
    if index.has_incoming_edge_outside_graph(&expected) {
        return None;
    }

    let mut edits_by_path: BTreeMap<PathBuf, Vec<Edit>> = BTreeMap::new();
    let mut fields_by_path: BTreeMap<PathBuf, Vec<(String, String)>> = BTreeMap::new();
    let mut names_by_path: BTreeMap<PathBuf, BTreeSet<String>> = BTreeMap::new();
    let mut changes = BTreeMap::new();
    let mut variants = BTreeMap::new();
    let mut structs_by_path: BTreeMap<PathBuf, BTreeMap<String, &GraphDeclaration>> =
        BTreeMap::new();
    for (name, (file, item, _)) in declarations {
        names_by_path
            .entry(file.path().to_path_buf())
            .or_default()
            .insert(name.clone());
        match item.kind() {
            DeclarationKind::Struct => {
                structs_by_path
                    .entry(file.path().to_path_buf())
                    .or_default()
                    .insert(name.clone(), item);
            }
            DeclarationKind::Enum => {
                for variant in item.variants() {
                    let field = variant.fields().iter().next()?;
                    if field.is_copy_shape() && (!field.is_reference() || field.is_unit_reference())
                    {
                        continue;
                    }
                    let (shape, _) = field.storage()?;
                    if let Some(replacement) = field.desired_type(
                        strategy,
                        item.only_lifetime(),
                        None,
                        graph_lifetime,
                        &changing_generics,
                    ) {
                        let (start, end) = field.type_range()?;
                        edits_by_path
                            .entry(file.path().to_path_buf())
                            .or_default()
                            .push(Edit {
                                start,
                                end,
                                replacement,
                            });
                        fields_by_path
                            .entry(file.path().to_path_buf())
                            .or_default()
                            .push((name.clone(), variant.name().to_string()));
                        variants.insert(
                            (name.clone(), variant.name().to_string()),
                            VariantChange { shape },
                        );
                    } else if field.wants_change(strategy == ErrorFieldStrategy::Borrowed) {
                        return None;
                    }
                }
            }
        }
    }
    for (path, structs) in &structs_by_path {
        let file = files.iter().find(|file| file.path() == path)?;
        let names: BTreeSet<_> = structs.keys().cloned().collect();
        let fields = plan_struct_fields(
            file,
            strategy,
            index,
            structs,
            &names,
            graph_lifetime,
            &changing_generics,
            true,
        );
        let (fields, edits, field_changes, valid) = fields.into_parts();
        if !valid {
            return None;
        }
        edits_by_path.entry(path.clone()).or_default().extend(edits);
        fields_by_path
            .entry(path.clone())
            .or_default()
            .extend(fields);
        changes.extend(field_changes);
    }

    let mut constructed = BTreeSet::new();
    let mut constructed_variants = BTreeSet::new();
    for file in files {
        if !expected.keys().any(|name| index.file_mentions(file, name)) {
            continue;
        }
        if !index.paths_resolve_to_graph(file, &expected) || index.name_blocks(file, "Box") {
            return None;
        }
        let mut visitor = GraphExprPlanner::new(
            file.source(),
            index.expression_facts(file)?,
            strategy,
            &changes,
            &variants,
        )
        .strict_external();
        if strategy == ErrorFieldStrategy::Borrowed {
            if let Some(lifetime) = &lifetime {
                visitor = visitor.require_anchored(lifetime.lifetime());
            }
        }
        visitor.run();
        let (edits, complete, found, found_variants) = visitor.finish();
        if !complete {
            return None;
        }
        let file_edits = edits_by_path.entry(file.path().to_path_buf()).or_default();
        file_edits.extend(edits);
        if let Some(lifetime) = &lifetime {
            let signature_edits =
                lifetime.distributed_edits(index.lifetime_facts(file)?, strategy, file_edits)?;
            file_edits.extend(signature_edits);
        }
        constructed.extend(found);
        constructed_variants.extend(found_variants);
    }
    if !changes.keys().all(|key| constructed.contains(key))
        || !variants
            .keys()
            .all(|key| constructed_variants.contains(key))
    {
        return None;
    }
    edits_by_path.retain(|_, edits| !edits.is_empty());
    let component_paths: BTreeSet<_> = edits_by_path.keys().cloned().collect();
    if component_paths.len() < 2 {
        return None;
    }
    let plans = edits_by_path
        .into_iter()
        .map(|(path, edits)| {
            let file = files.iter().find(|file| file.path() == path)?;
            Some(ErrorGraphPlan {
                path: path.clone(),
                source_digest: file.source_digest(),
                names: names_by_path.remove(&path).unwrap_or_default(),
                fields: fields_by_path.remove(&path).unwrap_or_default(),
                component_paths: component_paths.clone(),
                edits: Some(edits),
            })
        })
        .collect::<Option<Vec<_>>>()?;
    Some(plans)
}

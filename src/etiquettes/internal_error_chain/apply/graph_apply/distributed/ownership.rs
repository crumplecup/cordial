//! Discover connected declarations and retain ownership of rejected components.

use std::collections::{BTreeMap, BTreeSet};

use tracing::instrument;

use crate::config::ErrorFieldStrategy;
use crate::loader::SourceEditView;

use super::super::super::inventory::ErrorEdgeTargetKind;
use super::super::declarations::{DeclarationKind, GraphFields};
use super::{CrateTypeIndex, Declarations, ErrorEdgeTarget, ErrorGraphPlan, GraphDeclaration};

#[tracing::instrument(level = "debug", skip(declarations, strategy))]
pub(super) fn rejected_plans(
    declarations: &Declarations<'_>,
    strategy: ErrorFieldStrategy,
) -> Vec<ErrorGraphPlan> {
    let mut plans: BTreeMap<_, ErrorGraphPlan> = BTreeMap::new();
    for (name, (file, item, _)) in declarations {
        let plan = plans
            .entry(file.path().to_path_buf())
            .or_insert_with(|| ErrorGraphPlan {
                path: file.path().to_path_buf(),
                source_digest: file.source_digest(),
                names: Default::default(),
                fields: Vec::new(),
                component_paths: Default::default(),
                edits: None,
            });
        plan.names.insert(name.clone());
        for field in item.fields().iter() {
            if field.wants_change(strategy == ErrorFieldStrategy::Borrowed)
                && let Some(field_name) = field.name()
            {
                plan.fields.push((name.clone(), field_name.to_string()));
            }
        }
    }
    plans.into_values().collect()
}

#[instrument(level = "debug", skip(root_file, files, index))]
pub(super) fn discover<'a>(
    root_file: &SourceEditView,
    files: &'a [SourceEditView],
    index: &'a CrateTypeIndex,
) -> Option<BTreeMap<String, (&'a SourceEditView, &'a GraphDeclaration, ErrorEdgeTarget)>> {
    let roots: Vec<_> = index
        .graph_declarations(root_file)?
        .all()
        .iter()
        .filter_map(|item| {
            (item.kind() == DeclarationKind::Struct
                && item.error_impl()
                && item.is_inherited()
                && item
                    .fields()
                    .iter()
                    .any(|field| matches!(field.name(), Some("kind" | "source"))))
            .then(|| item.name().to_string())
        })
        .collect();
    let [root_name] = roots.as_slice() else {
        return None;
    };
    let root = index.resolve(root_file, std::slice::from_ref(root_name))?;
    let mut pending = vec![root];
    let mut visited = BTreeSet::new();
    let mut declarations: BTreeMap<String, (&SourceEditView, &GraphDeclaration, ErrorEdgeTarget)> =
        BTreeMap::new();
    let mut has_external_declaration = false;

    while let Some(target) = pending.pop() {
        let identity = (target.declaration().clone(), target.line(), target.column());
        if !visited.insert(identity) {
            continue;
        }
        let file = files.iter().find(|file| {
            file.path() == target.declaration() && file.source_digest() == target.source_digest()
        })?;
        let kind = match target.kind() {
            ErrorEdgeTargetKind::Struct => DeclarationKind::Struct,
            ErrorEdgeTargetKind::Enum => DeclarationKind::Enum,
        };
        let item = index
            .graph_declarations(file)?
            .at(target.line(), target.column(), kind)?;
        has_external_declaration |= file.path() != root_file.path();
        let name = item.name();
        if declarations
            .insert(name.to_string(), (file, item, target))
            .is_some()
        {
            return None;
        }
        match item.kind() {
            DeclarationKind::Struct => {
                if !item.attrs_allowed() || item.is_public() {
                    return None;
                }
                for field in item.fields().iter() {
                    let source = match field.name() {
                        Some("source") => true,
                        Some("kind") => false,
                        _ => continue,
                    };
                    let edge = field.edge(source)?;
                    match (source, index.resolve(file, edge.type_path())) {
                        (false, Some(next)) if next.kind() == ErrorEdgeTargetKind::Enum => {
                            pending.push(next);
                        }
                        (true, Some(next)) if next.kind() == ErrorEdgeTargetKind::Struct => {
                            pending.push(next);
                        }
                        (true, None)
                            if edge.type_path().first().is_some_and(|root| {
                                matches!(root.as_str(), "std" | "core" | "alloc")
                            }) => {}
                        _ => return None,
                    }
                }
            }
            DeclarationKind::Enum => {
                if !item.attrs_allowed() || item.is_public() {
                    return None;
                }
                for variant in item.variants() {
                    let GraphFields::Unnamed(fields) = variant.fields() else {
                        return None;
                    };
                    if fields.len() != 1 {
                        return None;
                    }
                    let field = fields.first()?;
                    if !variant.attrs_allowed() || !field.attrs_allowed() {
                        return None;
                    }
                    if field.is_copy_shape() && (!field.is_reference() || field.is_unit_reference())
                    {
                        continue;
                    }
                    let edge = field.edge(true)?;
                    if let Some(next) = index.resolve(file, edge.type_path()) {
                        if next.kind() == ErrorEdgeTargetKind::Struct {
                            pending.push(next);
                        }
                    } else if !edge.type_path().first().is_some_and(|root| {
                        matches!(root.as_str(), "std" | "core" | "alloc" | "String" | "str")
                    }) {
                        return None;
                    }
                }
            }
        }
    }
    has_external_declaration.then_some(declarations)
}

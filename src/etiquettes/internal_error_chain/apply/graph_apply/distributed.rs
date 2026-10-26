//! Plan a closed private error graph whose declarations span source files.
//!
//! Every declaration comes from the crate's shared parsed files through the
//! same identity index used by conversion-unit enrichment. Unknown edges,
//! hidden names, and unsupported lifetime changes leave the graph untouched.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use syn::spanned::Spanned;
use syn::{Item, Type, Visibility};
use tracing::instrument;

use crate::config::ErrorFieldStrategy;
use crate::loader::SharedParsedFile;

use super::super::inventory::{
    CrateTypeIndex, ErrorEdgeField, ErrorEdgeTarget, ErrorEdgeTargetKind, error_edge_evidence,
};
use super::super::{Edit, known_error_impls, span_range};
use super::expressions::GraphExprPlanner;
use super::lifetimes;
use super::plan::plan_struct_fields;
use super::types::{allowed_attrs, desired_type, is_copy_shape, storage_shape, wants_change};
use super::{ErrorGraphPlan, VariantChange};

/// Return complete per-file plans only when one root reaches an external Kind
/// or source declaration and all connected edits can be proven together.
#[instrument(level = "debug", skip(root_file, files, strategy, index))]
pub(super) fn plan(
    root_file: &SharedParsedFile,
    files: &[SharedParsedFile],
    strategy: ErrorFieldStrategy,
    index: &CrateTypeIndex,
) -> Option<Vec<ErrorGraphPlan>> {
    let declarations = discover_declarations(root_file, files, index)?;
    let expected: BTreeMap<String, ErrorEdgeTarget> = declarations
        .iter()
        .map(|(name, (_, _, target))| (name.clone(), target.clone()))
        .collect();
    let names: BTreeSet<_> = declarations.keys().cloned().collect();
    let structs: BTreeMap<_, _> = declarations
        .iter()
        .filter_map(|(name, (_, item, _))| match item {
            Item::Struct(item) => Some((name.clone(), item)),
            _ => None,
        })
        .collect();
    let enums: BTreeMap<_, _> = declarations
        .iter()
        .filter_map(|(name, (_, item, _))| match item {
            Item::Enum(item) => Some((name.clone(), item)),
            _ => None,
        })
        .collect();
    let lifetime = lifetimes::select_across(strategy, &names, &structs, &enums, index);
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
    let mut structs_by_path: BTreeMap<PathBuf, BTreeMap<String, &syn::ItemStruct>> =
        BTreeMap::new();
    for (name, (file, item, _)) in &declarations {
        names_by_path
            .entry(file.path().to_path_buf())
            .or_default()
            .insert(name.clone());
        match item {
            Item::Struct(item) => {
                structs_by_path
                    .entry(file.path().to_path_buf())
                    .or_default()
                    .insert(name.clone(), item);
            }
            Item::Enum(item) => {
                for variant in &item.variants {
                    let field = variant.fields.iter().next()?;
                    if is_copy_shape(&field.ty)
                        && (!matches!(field.ty, Type::Reference(_))
                            || matches!(&field.ty, Type::Reference(reference)
                                if matches!(&*reference.elem, Type::Tuple(tuple) if tuple.elems.is_empty())))
                    {
                        continue;
                    }
                    let (shape, _) = storage_shape(&field.ty)?;
                    if let Some(replacement) = desired_type(
                        file,
                        &field.ty,
                        strategy,
                        item,
                        None,
                        graph_lifetime,
                        &changing_generics,
                    ) {
                        let (start, end) = span_range(file.source(), field.ty.span())?;
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
                            .push((name.clone(), variant.ident.to_string()));
                        variants.insert(
                            (name.clone(), variant.ident.to_string()),
                            VariantChange { shape },
                        );
                    } else if wants_change(&field.ty, strategy) {
                        return None;
                    }
                }
            }
            _ => return None,
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

#[instrument(level = "debug", skip(root_file, files, index))]
fn discover_declarations<'a>(
    root_file: &SharedParsedFile,
    files: &'a [SharedParsedFile],
    index: &CrateTypeIndex,
) -> Option<BTreeMap<String, (&'a SharedParsedFile, &'a Item, ErrorEdgeTarget)>> {
    let error_impls = known_error_impls(root_file.syntax());
    let roots: Vec<_> = root_file
        .syntax()
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Struct(item)
                if error_impls.contains(&item.ident.to_string())
                    && matches!(item.vis, Visibility::Inherited)
                    && item.fields.iter().any(|field| {
                        field
                            .ident
                            .as_ref()
                            .is_some_and(|ident| ident == "kind" || ident == "source")
                    }) =>
            {
                Some(item.ident.to_string())
            }
            _ => None,
        })
        .collect();
    let [root_name] = roots.as_slice() else {
        return None;
    };
    let root = index.resolve(root_file, std::slice::from_ref(root_name))?;
    let mut pending = vec![root];
    let mut visited = BTreeSet::new();
    let mut declarations: BTreeMap<String, (&SharedParsedFile, &Item, ErrorEdgeTarget)> =
        BTreeMap::new();
    let mut has_external_declaration = false;

    while let Some(target) = pending.pop() {
        let identity = (target.declaration().clone(), target.line(), target.column());
        if !visited.insert(identity) {
            continue;
        }
        let (file, item) = index.declaration(files, &target)?;
        has_external_declaration |= file.path() != root_file.path();
        let name = match item {
            Item::Struct(item) => &item.ident,
            Item::Enum(item) => &item.ident,
            _ => return None,
        };
        if declarations
            .insert(name.to_string(), (file, item, target))
            .is_some()
        {
            return None;
        }
        match item {
            Item::Struct(item) => {
                if !allowed_attrs(&item.attrs) || matches!(item.vis, Visibility::Public(_)) {
                    return None;
                }
                for field in &item.fields {
                    let edge_field = match field.ident.as_ref().map(ToString::to_string).as_deref()
                    {
                        Some("source") => ErrorEdgeField::Source,
                        Some("kind") => ErrorEdgeField::Kind,
                        _ => continue,
                    };
                    let edge = error_edge_evidence(file, field, edge_field)?;
                    match (edge_field, index.resolve(file, edge.type_path())) {
                        (ErrorEdgeField::Kind, Some(next))
                            if next.kind() == ErrorEdgeTargetKind::Enum =>
                        {
                            pending.push(next);
                        }
                        (ErrorEdgeField::Source, Some(next))
                            if next.kind() == ErrorEdgeTargetKind::Struct =>
                        {
                            pending.push(next);
                        }
                        (ErrorEdgeField::Source, None)
                            if edge.type_path().first().is_some_and(|root| {
                                matches!(root.as_str(), "std" | "core" | "alloc")
                            }) => {}
                        _ => return None,
                    }
                }
            }
            Item::Enum(item) => {
                if !allowed_attrs(&item.attrs) || matches!(item.vis, Visibility::Public(_)) {
                    return None;
                }
                for variant in &item.variants {
                    let syn::Fields::Unnamed(fields) = &variant.fields else {
                        return None;
                    };
                    if fields.unnamed.len() != 1 {
                        return None;
                    }
                    let field = fields.unnamed.first()?;
                    if !allowed_attrs(&variant.attrs) || !allowed_attrs(&field.attrs) {
                        return None;
                    }
                    if is_copy_shape(&field.ty)
                        && (!matches!(field.ty, Type::Reference(_))
                            || matches!(&field.ty, Type::Reference(reference)
                                if matches!(&*reference.elem, Type::Tuple(tuple) if tuple.elems.is_empty())))
                    {
                        continue;
                    }
                    let edge = error_edge_evidence(file, field, ErrorEdgeField::Source)?;
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
            _ => return None,
        }
    }
    has_external_declaration.then_some(declarations)
}

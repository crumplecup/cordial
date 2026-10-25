//! Discover one closed error graph and plan its declaration and constructor edits.

use std::collections::{BTreeMap, BTreeSet};

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{Item, Type, Visibility};
use tracing::instrument;

use crate::config::ErrorFieldStrategy;
use crate::loader::SharedParsedFile;

use super::super::inventory::{CrateTypeIndex, ErrorEdgeField, error_edge_evidence};
use super::super::names::audit_names;
use super::super::{Edit, known_error_impls, span_range};
use super::expressions::GraphExprPlanner;
use super::lifetimes;
use super::types::{
    allowed_attrs, desired_type, is_copy_shape, storage_shape, type_name, wants_change,
};
use super::{Change, ErrorGraphPlan, GraphCandidate, Shape, VariantChange};

#[instrument(level = "debug", skip(file, strategy, index))]
pub(super) fn plan_file(
    file: &SharedParsedFile,
    strategy: ErrorFieldStrategy,
    index: &CrateTypeIndex,
) -> Option<GraphCandidate> {
    let syntax = file.syntax();
    let impls = known_error_impls(syntax);
    let structs: BTreeMap<_, _> = syntax
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Struct(item) => Some((item.ident.to_string(), item)),
            _ => None,
        })
        .collect();
    let enums: BTreeMap<_, _> = syntax
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Enum(item) => Some((item.ident.to_string(), item)),
            _ => None,
        })
        .collect();
    let mut roots = Vec::new();
    for (name, item) in &structs {
        if !impls.contains(name) || !matches!(item.vis, Visibility::Inherited) {
            continue;
        }
        let syn::Fields::Named(fields) = &item.fields else {
            continue;
        };
        if fields.named.iter().any(|field| {
            field
                .ident
                .as_ref()
                .is_some_and(|id| id == "kind" || id == "source")
        }) {
            roots.push(name.clone());
        }
    }
    if roots.is_empty() {
        return None;
    }

    let initial = discover_graph(
        file,
        strategy,
        &structs,
        &enums,
        roots.clone(),
        None,
        &BTreeSet::new(),
    );
    let lifetime = lifetimes::select(file, strategy, &initial.0, &structs, &enums);
    let changing_generics = lifetime
        .as_ref()
        .map(|plan| plan.names().clone())
        .unwrap_or_default();
    let graph_lifetime = lifetime.as_ref().map(|plan| plan.lifetime().as_str());
    let (names, mut fields, mut edits, variants, mut valid) = if lifetime.is_some() {
        discover_graph(
            file,
            strategy,
            &structs,
            &enums,
            roots,
            graph_lifetime,
            &changing_generics,
        )
    } else {
        initial
    };
    let field_plan = plan_struct_fields(
        file,
        strategy,
        index,
        &structs,
        &names,
        graph_lifetime,
        &changing_generics,
    );
    fields.extend(field_plan.fields);
    edits.extend(field_plan.edits);
    valid &= field_plan.valid;
    let changes = field_plan.changes;

    let mut visitor = GraphExprPlanner::new(file, strategy, &changes, &variants);
    visitor.visit_file(syntax);
    let (expr_edits, expr_valid, constructed, constructed_variants) = visitor.finish();
    valid &= expr_valid;
    let audit = audit_names(syntax);
    valid &= names.iter().all(|name| !audit.blocks(name));
    valid &= !audit.blocks("Box");
    edits.extend(expr_edits);
    let lifetime_transition = lifetime.is_some();
    if let Some(lifetime) = lifetime {
        if let Some(mut lifetime_edits) = lifetime.edits(file, strategy, &edits) {
            edits.append(&mut lifetime_edits);
        } else {
            valid = false;
        }
    }
    if fields.is_empty() {
        return None;
    }
    Some(GraphCandidate {
        plan: ErrorGraphPlan {
            path: file.path().to_path_buf(),
            source_digest: file.source_digest(),
            names,
            fields,
            component_paths: BTreeSet::new(),
            edits: valid.then_some(edits),
        },
        changes,
        variants,
        constructed,
        constructed_variants,
        lifetime_transition,
    })
}

#[instrument(
    level = "debug",
    skip(file, strategy, structs, enums, roots, changing_generics)
)]
fn discover_graph<'a>(
    file: &SharedParsedFile,
    strategy: ErrorFieldStrategy,
    structs: &BTreeMap<String, &'a syn::ItemStruct>,
    enums: &BTreeMap<String, &'a syn::ItemEnum>,
    roots: Vec<String>,
    graph_lifetime: Option<&str>,
    changing_generics: &BTreeSet<String>,
) -> (
    BTreeSet<String>,
    Vec<(String, String)>,
    Vec<Edit>,
    BTreeMap<(String, String), VariantChange>,
    bool,
) {
    let mut names = BTreeSet::new();
    let mut fields = Vec::new();
    let mut edits = Vec::new();
    let mut variants = BTreeMap::new();
    let mut valid = true;
    let mut pending = roots;
    let mut visited_enums = BTreeSet::new();
    while let Some(name) = pending.pop() {
        if !names.insert(name.clone()) {
            continue;
        }
        let Some(item) = structs.get(&name) else {
            valid = false;
            continue;
        };
        if !allowed_attrs(&item.attrs) {
            valid = false;
        }
        let named = match &item.fields {
            syn::Fields::Named(named) => named,
            syn::Fields::Unit => continue,
            syn::Fields::Unnamed(_) => {
                valid = false;
                continue;
            }
        };
        for field in &named.named {
            let field_name = field.ident.as_ref().map(ToString::to_string);
            if field_name.as_deref() == Some("source") {
                if let Some(target) = type_name(&field.ty) {
                    if structs.contains_key(&target) {
                        pending.push(target);
                    }
                }
            }
            if field_name.as_deref() != Some("kind") {
                continue;
            }
            let Some(kind_name) = type_name(&field.ty) else {
                valid = false;
                continue;
            };
            let Some(kind) = enums.get(&kind_name) else {
                valid = false;
                continue;
            };
            if !matches!(kind.vis, Visibility::Inherited) {
                valid = false;
                continue;
            }
            if !allowed_attrs(&kind.attrs) {
                valid = false;
            }
            names.insert(kind_name.clone());
            if !visited_enums.insert(kind_name.clone()) {
                continue;
            }
            for variant in &kind.variants {
                let syn::Fields::Unnamed(fields_in_variant) = &variant.fields else {
                    valid = false;
                    continue;
                };
                if fields_in_variant.unnamed.len() != 1 {
                    valid = false;
                    continue;
                }
                let field = &fields_in_variant.unnamed[0];
                if !allowed_attrs(&variant.attrs) || !allowed_attrs(&field.attrs) {
                    valid = false;
                }
                if is_copy_shape(&field.ty) && !matches!(field.ty, Type::Reference(_)) {
                    continue;
                }
                let Some((shape, payload)) = storage_shape(&field.ty) else {
                    valid = false;
                    continue;
                };
                if let Some(source) = structs.get(&payload) {
                    if matches!(source.vis, Visibility::Inherited) {
                        pending.push(payload.clone());
                    }
                }
                if let Some(replacement) = desired_type(
                    file,
                    &field.ty,
                    strategy,
                    *kind,
                    None,
                    graph_lifetime,
                    changing_generics,
                ) {
                    if let Some((start, end)) = span_range(file.source(), field.ty.span()) {
                        edits.push(Edit {
                            start,
                            end,
                            replacement,
                        });
                        variants.insert(
                            (kind_name.clone(), variant.ident.to_string()),
                            VariantChange { shape },
                        );
                        fields.push((kind_name.clone(), variant.ident.to_string()));
                    } else {
                        valid = false;
                    }
                } else if wants_change(&field.ty, strategy) {
                    valid = false;
                    fields.push((kind_name.clone(), variant.ident.to_string()));
                }
            }
        }
    }

    (names, fields, edits, variants, valid)
}

struct FieldPlan {
    fields: Vec<(String, String)>,
    edits: Vec<Edit>,
    changes: BTreeMap<(String, String), Change>,
    valid: bool,
}

#[instrument(
    level = "debug",
    skip(file, strategy, index, structs, names, changing_generics)
)]
fn plan_struct_fields(
    file: &SharedParsedFile,
    strategy: ErrorFieldStrategy,
    index: &CrateTypeIndex,
    structs: &BTreeMap<String, &syn::ItemStruct>,
    names: &BTreeSet<String>,
    graph_lifetime: Option<&str>,
    changing_generics: &BTreeSet<String>,
) -> FieldPlan {
    let mut fields = Vec::new();
    let mut edits = Vec::new();
    let mut valid = true;
    let mut changes = BTreeMap::new();

    for name in names {
        let Some(item) = structs.get(name) else {
            continue;
        };
        for field in &item.fields {
            let edge_field = match field.ident.as_ref().map(ToString::to_string).as_deref() {
                Some("source") => ErrorEdgeField::Source,
                Some("kind") => ErrorEdgeField::Kind,
                _ => continue,
            };
            let Some(edge) = error_edge_evidence(file, field, edge_field) else {
                valid = false;
                continue;
            };
            match index.resolve(file, edge.type_path()) {
                Some(target) if target.declaration() != file.path() => valid = false,
                None if edge_field == ErrorEdgeField::Kind
                    || !edge
                        .type_path()
                        .first()
                        .is_some_and(|root| matches!(root.as_str(), "std" | "core" | "alloc")) =>
                {
                    valid = false;
                }
                _ => {}
            }
        }
    }

    for name in names {
        let Some(item) = structs.get(name) else {
            continue;
        };
        if !matches!(item.vis, Visibility::Inherited) {
            valid = false;
            continue;
        }
        let named = match &item.fields {
            syn::Fields::Named(named) => named,
            syn::Fields::Unit => continue,
            syn::Fields::Unnamed(_) => {
                valid = false;
                continue;
            }
        };
        for field in &named.named {
            let Some(field_name) = field.ident.as_ref().map(ToString::to_string) else {
                valid = false;
                continue;
            };
            if !matches!(field.vis, Visibility::Inherited) {
                valid = false;
                continue;
            }
            if !allowed_attrs(&field.attrs) {
                valid = false;
            }
            if is_copy_shape(&field.ty)
                && !matches!(&field.ty, Type::Reference(reference)
                if field_name == "source" || field_name == "kind" || type_name(&reference.elem).as_deref() == Some("str"))
            {
                continue;
            }
            let Some((shape, _inner)) = storage_shape(&field.ty) else {
                if !is_copy_shape(&field.ty) {
                    valid = false;
                }
                continue;
            };
            let Some(replacement) = desired_type(
                file,
                &field.ty,
                strategy,
                *item,
                Some(&field_name),
                graph_lifetime,
                &changing_generics,
            ) else {
                if wants_change(&field.ty, strategy) {
                    valid = false;
                    fields.push((name.clone(), field_name.clone()));
                }
                continue;
            };
            if let Some((start, end)) = span_range(file.source(), field.ty.span()) {
                edits.push(Edit {
                    start,
                    end,
                    replacement,
                });
                changes.insert(
                    (name.clone(), field_name.clone()),
                    Change {
                        shape: if field_name == "kind" && strategy == ErrorFieldStrategy::Owned {
                            Shape::Boxed
                        } else {
                            shape
                        },
                    },
                );
                fields.push((name.clone(), field_name));
            } else {
                valid = false;
            }
        }
    }

    FieldPlan {
        fields,
        edits,
        changes,
        valid,
    }
}

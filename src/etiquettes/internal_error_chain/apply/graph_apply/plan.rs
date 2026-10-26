//! Discover one closed error graph and plan its declaration and constructor edits.

use std::collections::{BTreeMap, BTreeSet};

use tracing::instrument;

use crate::config::ErrorFieldStrategy;
use crate::loader::SourceEditView;

use super::super::Edit;
use super::super::inventory::CrateTypeIndex;
use super::declarations::{DeclarationKind, GraphDeclaration, GraphFields};
use super::expressions::GraphExprPlanner;
use super::lifetimes;
use super::{Change, ErrorGraphPlan, GraphCandidate, Shape, VariantChange};

#[instrument(level = "debug", skip(file, strategy, index))]
pub(super) fn plan_file(
    file: &SourceEditView,
    strategy: ErrorFieldStrategy,
    index: &CrateTypeIndex,
) -> Option<GraphCandidate> {
    let declarations = index.graph_declarations(file)?;
    let structs: BTreeMap<_, _> = declarations
        .all()
        .iter()
        .filter(|item| item.kind() == DeclarationKind::Struct)
        .map(|item| (item.name().to_string(), item))
        .collect();
    let enums: BTreeMap<_, _> = declarations
        .all()
        .iter()
        .filter(|item| item.kind() == DeclarationKind::Enum)
        .map(|item| (item.name().to_string(), item))
        .collect();
    let mut roots = Vec::new();
    for (name, item) in &structs {
        if !item.error_impl() || !item.is_inherited() {
            continue;
        }
        let GraphFields::Named(fields) = item.fields() else {
            continue;
        };
        if fields
            .iter()
            .any(|field| matches!(field.name(), Some("kind" | "source")))
        {
            roots.push(name.clone());
        }
    }
    if roots.is_empty() {
        return None;
    }

    let initial = discover_graph(
        strategy,
        &structs,
        &enums,
        roots.clone(),
        None,
        &BTreeSet::new(),
    );
    let lifetime = lifetimes::select(file, strategy, &initial.0, index);
    let changing_generics = lifetime
        .as_ref()
        .map(|plan| plan.names().clone())
        .unwrap_or_default();
    let graph_lifetime = lifetime.as_ref().map(|plan| plan.lifetime().as_str());
    let (names, mut fields, mut edits, variants, mut valid) = if lifetime.is_some() {
        discover_graph(
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
        false,
    );
    fields.extend(field_plan.fields);
    edits.extend(field_plan.edits);
    valid &= field_plan.valid;
    let changes = field_plan.changes;

    let mut visitor = GraphExprPlanner::new(
        file.source(),
        index.expression_facts(file)?,
        strategy,
        &changes,
        &variants,
    );
    visitor.run();
    let (expr_edits, expr_valid, constructed, constructed_variants) = visitor.finish();
    valid &= expr_valid;
    valid &= names.iter().all(|name| !index.name_blocks(file, name));
    valid &= !index.name_blocks(file, "Box");
    edits.extend(expr_edits);
    let lifetime_transition = lifetime.is_some();
    if let Some(lifetime) = lifetime {
        if let Some(mut lifetime_edits) =
            lifetime.edits(index.lifetime_facts(file)?, strategy, &edits)
        {
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
    skip(strategy, structs, enums, roots, changing_generics)
)]
fn discover_graph<'a>(
    strategy: ErrorFieldStrategy,
    structs: &BTreeMap<String, &'a GraphDeclaration>,
    enums: &BTreeMap<String, &'a GraphDeclaration>,
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
        if !item.attrs_allowed() {
            valid = false;
        }
        let named = match item.fields() {
            GraphFields::Named(named) => named,
            GraphFields::Unit => continue,
            GraphFields::Unnamed(_) => {
                valid = false;
                continue;
            }
        };
        for field in named {
            let field_name = field.name();
            if field_name.as_deref() == Some("source") {
                if let Some(target) = field.type_name() {
                    if structs.contains_key(target) {
                        pending.push(target.to_string());
                    }
                }
            }
            if field_name != Some("kind") {
                continue;
            }
            let Some(kind_name) = field.type_name() else {
                valid = false;
                continue;
            };
            let Some(kind) = enums.get(kind_name) else {
                valid = false;
                continue;
            };
            if !kind.is_inherited() {
                valid = false;
                continue;
            }
            if !kind.attrs_allowed() {
                valid = false;
            }
            names.insert(kind_name.to_string());
            if !visited_enums.insert(kind_name.to_string()) {
                continue;
            }
            for variant in kind.variants() {
                let GraphFields::Unnamed(fields_in_variant) = variant.fields() else {
                    valid = false;
                    continue;
                };
                if fields_in_variant.len() != 1 {
                    valid = false;
                    continue;
                }
                let field = &fields_in_variant[0];
                if !variant.attrs_allowed() || !field.attrs_allowed() {
                    valid = false;
                }
                if field.is_copy_shape() && !field.is_reference() {
                    continue;
                }
                let Some((shape, payload)) = field.storage() else {
                    valid = false;
                    continue;
                };
                if let Some(source) = structs.get(payload) {
                    if source.is_inherited() {
                        pending.push(payload.to_string());
                    }
                }
                if let Some(replacement) = field.desired_type(
                    strategy,
                    kind.only_lifetime(),
                    None,
                    graph_lifetime,
                    changing_generics,
                ) {
                    if let Some((start, end)) = field.type_range() {
                        edits.push(Edit {
                            start,
                            end,
                            replacement,
                        });
                        variants.insert(
                            (kind_name.to_string(), variant.name().to_string()),
                            VariantChange { shape },
                        );
                        fields.push((kind_name.to_string(), variant.name().to_string()));
                    } else {
                        valid = false;
                    }
                } else if field.wants_change(strategy == ErrorFieldStrategy::Borrowed) {
                    valid = false;
                    fields.push((kind_name.to_string(), variant.name().to_string()));
                }
            }
        }
    }

    (names, fields, edits, variants, valid)
}

pub(super) struct FieldPlan {
    fields: Vec<(String, String)>,
    edits: Vec<Edit>,
    changes: BTreeMap<(String, String), Change>,
    valid: bool,
}

impl FieldPlan {
    #[instrument(level = "trace", skip(self))]
    pub(super) fn into_parts(
        self,
    ) -> (
        Vec<(String, String)>,
        Vec<Edit>,
        BTreeMap<(String, String), Change>,
        bool,
    ) {
        (self.fields, self.edits, self.changes, self.valid)
    }
}

#[instrument(
    level = "debug",
    skip(file, strategy, index, structs, names, changing_generics)
)]
pub(super) fn plan_struct_fields(
    file: &SourceEditView,
    strategy: ErrorFieldStrategy,
    index: &CrateTypeIndex,
    structs: &BTreeMap<String, &GraphDeclaration>,
    names: &BTreeSet<String>,
    graph_lifetime: Option<&str>,
    changing_generics: &BTreeSet<String>,
    allow_external: bool,
) -> FieldPlan {
    let mut fields = Vec::new();
    let mut edits = Vec::new();
    let mut valid = true;
    let mut changes = BTreeMap::new();

    for name in names {
        let Some(item) = structs.get(name) else {
            continue;
        };
        for field in item.fields().iter() {
            let source = match field.name() {
                Some("source") => true,
                Some("kind") => false,
                _ => continue,
            };
            let Some(edge) = field.edge(source) else {
                valid = false;
                continue;
            };
            match index.resolve(file, edge.type_path()) {
                Some(target) if !allow_external && target.declaration() != file.path() => {
                    valid = false;
                }
                None if !source
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
        if item.is_public() || (!allow_external && !item.is_inherited()) {
            valid = false;
            continue;
        }
        let named = match item.fields() {
            GraphFields::Named(named) => named,
            GraphFields::Unit => continue,
            GraphFields::Unnamed(_) => {
                valid = false;
                continue;
            }
        };
        for field in named {
            let Some(field_name) = field.name() else {
                valid = false;
                continue;
            };
            if field.is_public() || (!allow_external && !field.is_inherited()) {
                valid = false;
                continue;
            }
            if !field.attrs_allowed() {
                valid = false;
            }
            if field.is_copy_shape()
                && !(field.is_reference()
                    && (field_name == "source"
                        || field_name == "kind"
                        || field.type_name() == Some("str")))
            {
                continue;
            }
            let Some((shape, _inner)) = field.storage() else {
                if !field.is_copy_shape() {
                    valid = false;
                }
                continue;
            };
            let Some(replacement) = field.desired_type(
                strategy,
                item.only_lifetime(),
                Some(field_name),
                graph_lifetime,
                &changing_generics,
            ) else {
                if field.wants_change(strategy == ErrorFieldStrategy::Borrowed) {
                    valid = false;
                    fields.push((name.clone(), field_name.to_string()));
                }
                continue;
            };
            if let Some((start, end)) = field.type_range() {
                edits.push(Edit {
                    start,
                    end,
                    replacement,
                });
                changes.insert(
                    (name.clone(), field_name.to_string()),
                    Change {
                        shape: if field_name == "kind" && strategy == ErrorFieldStrategy::Owned {
                            Shape::Boxed
                        } else {
                            shape
                        },
                    },
                );
                fields.push((name.clone(), field_name.to_string()));
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

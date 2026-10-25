//! Whole-file plans for a private error, its Kind, and native source fields.
//!
//! The planner uses the shared parsed tree. A graph is edited only when its
//! declarations, construction expressions, and storage-sensitive reads all
//! have a supported rewrite.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{Item, Type, Visibility};
use tracing::instrument;

use crate::config::ErrorFieldStrategy;
use crate::error::{CordialError, CordialResult};
use crate::ir::{IrMut, IrView};
use crate::loader::SharedParsedFile;

use super::names::audit_names;
use super::{Edit, known_error_impls, span_range};

mod expressions;
mod types;

use expressions::GraphExprPlanner;
use types::{allowed_attrs, desired_type, is_copy_shape, storage_shape, type_name, wants_change};

const GRAPH_ATTR: &str = "error_apply_graph_plans";

#[derive(Debug, Clone, Serialize, Deserialize, derive_getters::Getters)]
pub(super) struct ErrorGraphPlan {
    path: PathBuf,
    #[getter(copy)]
    source_digest: [u8; 32],
    #[getter(skip)]
    names: BTreeSet<String>,
    fields: Vec<(String, String)>,
    #[getter(skip)]
    edits: Option<Vec<Edit>>,
}

impl ErrorGraphPlan {
    #[tracing::instrument(level = "trace", skip(self))]
    pub(super) fn edits(&self) -> Option<&[Edit]> {
        self.edits.as_deref()
    }
    #[tracing::instrument(level = "trace", skip(self))]
    pub(super) fn covers(&self, name: &str) -> bool {
        self.names.contains(name)
    }
}

#[instrument(level = "debug", skip(ir, files, strategy))]
pub(super) fn enrich_conversion_graphs(
    ir: &mut dyn IrMut,
    files: &[SharedParsedFile],
    strategy: ErrorFieldStrategy,
) -> CordialResult<()> {
    let mut plans: Vec<_> = files
        .iter()
        .filter_map(|file| plan_file(file, strategy))
        .collect();
    if !plans.is_empty() {
        let identifiers: Vec<_> = files
            .iter()
            .map(|file| {
                let mut names = CrossFileNames::default();
                names.visit_file(file.syntax());
                names.0
            })
            .collect();
        for plan in &mut plans {
            if files.iter().zip(&identifiers).any(|(file, other)| {
                file.path() != plan.path()
                    && plan.names.iter().any(|name| other.contains(name.as_str()))
            }) {
                plan.edits = None;
            }
        }
    }
    let root = ir.root()?;
    ir.set_attr(root, GRAPH_ATTR, serde_json::to_value(plans)?)?;
    Ok(())
}

#[derive(Default)]
struct CrossFileNames(BTreeSet<String>);

impl<'ast> Visit<'ast> for CrossFileNames {
    #[instrument(level = "debug", skip(self, ident))]
    fn visit_ident(&mut self, ident: &'ast proc_macro2::Ident) {
        self.0.insert(ident.to_string());
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_macro(&mut self, item: &'ast syn::Macro) {
        collect_token_idents(&item.tokens, &mut self.0);
    }
}

#[instrument(level = "debug", skip(tokens, names))]
fn collect_token_idents(tokens: &proc_macro2::TokenStream, names: &mut BTreeSet<String>) {
    for tree in tokens.clone() {
        match tree {
            proc_macro2::TokenTree::Ident(ident) => {
                names.insert(ident.to_string());
            }
            proc_macro2::TokenTree::Group(group) => collect_token_idents(&group.stream(), names),
            _ => {}
        }
    }
}

#[instrument(level = "debug", skip(ir))]
pub(super) fn conversion_graphs_from_ir(ir: &dyn IrView) -> CordialResult<Vec<ErrorGraphPlan>> {
    let root = ir.root()?;
    let value = ir
        .node(root)
        .and_then(|node| node.attr(GRAPH_ATTR).cloned())
        .ok_or_else(|| CordialError::unreachable(GRAPH_ATTR))?;
    Ok(serde_json::from_value(value)?)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    String,
    Value,
    Boxed,
}

#[derive(Debug, Clone)]
struct Change {
    shape: Shape,
}

#[derive(Debug, Clone)]
struct VariantChange {
    shape: Shape,
}

#[instrument(level = "debug", skip(file, strategy))]
fn plan_file(file: &SharedParsedFile, strategy: ErrorFieldStrategy) -> Option<ErrorGraphPlan> {
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

    let (names, mut fields, mut edits, variants, mut valid) =
        discover_graph(file, strategy, &structs, &enums, roots);
    let mut changes = BTreeMap::new();

    for name in &names {
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
            let Some(replacement) =
                desired_type(file, &field.ty, strategy, *item, Some(&field_name))
            else {
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

    let mut visitor = GraphExprPlanner::new(file, strategy, &changes, &variants);
    visitor.visit_file(syntax);
    let (expr_edits, expr_valid, constructed, constructed_variants) = visitor.finish();
    valid &= expr_valid;
    valid &= changes.keys().all(|key| constructed.contains(key));
    valid &= variants
        .keys()
        .all(|key| constructed_variants.contains(key));
    let audit = audit_names(syntax);
    valid &= names.iter().all(|name| !audit.blocks(name));
    valid &= !audit.blocks("Box");
    edits.extend(expr_edits);
    if fields.is_empty() {
        return None;
    }
    Some(ErrorGraphPlan {
        path: file.path().to_path_buf(),
        source_digest: file.source_digest(),
        names,
        fields,
        edits: valid.then_some(edits),
    })
}

#[instrument(level = "debug", skip(file, strategy, structs, enums, roots))]
fn discover_graph<'a>(
    file: &SharedParsedFile,
    strategy: ErrorFieldStrategy,
    structs: &BTreeMap<String, &'a syn::ItemStruct>,
    enums: &BTreeMap<String, &'a syn::ItemEnum>,
    roots: Vec<String>,
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
                if let Some(replacement) = desired_type(file, &field.ty, strategy, *kind, None) {
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

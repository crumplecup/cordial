//! Walk rustdoc generic args, bounds, and where-predicates.

use std::collections::HashSet;

use tracing::instrument;

use super::ExtractedItem;
use super::type_walk::{collect_items_from_path, collect_items_from_type};

#[instrument(skip(krate, args, own_crate_key, prefix_match, seen, discovered))]
pub(super) fn collect_items_from_generic_args(
    krate: &rustdoc_types::Crate,
    args: &rustdoc_types::GenericArgs,
    own_crate_key: &str,
    prefix_match: bool,
    seen: &mut HashSet<String>,
    discovered: &mut Vec<ExtractedItem>,
) {
    match args {
        rustdoc_types::GenericArgs::AngleBracketed { args, constraints } => {
            for arg in args {
                if let rustdoc_types::GenericArg::Type(ty) = arg {
                    collect_items_from_type(
                        krate,
                        ty,
                        own_crate_key,
                        prefix_match,
                        seen,
                        discovered,
                    );
                }
            }
            for constraint in constraints {
                if let Some(args) = &constraint.args {
                    collect_items_from_generic_args(
                        krate,
                        args,
                        own_crate_key,
                        prefix_match,
                        seen,
                        discovered,
                    );
                }
                match &constraint.binding {
                    rustdoc_types::AssocItemConstraintKind::Equality(term) => {
                        collect_items_from_term(
                            krate,
                            term,
                            own_crate_key,
                            prefix_match,
                            seen,
                            discovered,
                        );
                    }
                    rustdoc_types::AssocItemConstraintKind::Constraint(bounds) => {
                        for bound in bounds {
                            collect_items_from_generic_bound(
                                krate,
                                bound,
                                own_crate_key,
                                prefix_match,
                                seen,
                                discovered,
                            );
                        }
                    }
                }
            }
        }
        rustdoc_types::GenericArgs::Parenthesized { inputs, output } => {
            for input in inputs {
                collect_items_from_type(
                    krate,
                    input,
                    own_crate_key,
                    prefix_match,
                    seen,
                    discovered,
                );
            }
            if let Some(output) = output {
                collect_items_from_type(
                    krate,
                    output,
                    own_crate_key,
                    prefix_match,
                    seen,
                    discovered,
                );
            }
        }
        rustdoc_types::GenericArgs::ReturnTypeNotation => {}
    }
}
#[instrument(skip(krate, term, own_crate_key, prefix_match, seen, discovered))]
fn collect_items_from_term(
    krate: &rustdoc_types::Crate,
    term: &rustdoc_types::Term,
    own_crate_key: &str,
    prefix_match: bool,
    seen: &mut HashSet<String>,
    discovered: &mut Vec<ExtractedItem>,
) {
    if let rustdoc_types::Term::Type(ty) = term {
        collect_items_from_type(krate, ty, own_crate_key, prefix_match, seen, discovered);
    }
}
#[instrument(skip(krate, bound, own_crate_key, prefix_match, seen, discovered))]
pub(super) fn collect_items_from_generic_bound(
    krate: &rustdoc_types::Crate,
    bound: &rustdoc_types::GenericBound,
    own_crate_key: &str,
    prefix_match: bool,
    seen: &mut HashSet<String>,
    discovered: &mut Vec<ExtractedItem>,
) {
    if let rustdoc_types::GenericBound::TraitBound {
        trait_,
        generic_params,
        ..
    } = bound
    {
        collect_items_from_path(krate, trait_, own_crate_key, prefix_match, seen, discovered);
        collect_items_from_generic_param_defs(
            krate,
            generic_params,
            own_crate_key,
            prefix_match,
            seen,
            discovered,
        );
    }
}
#[instrument(skip(krate, poly_trait, seen, discovered), fields(path = %poly_trait.trait_.path))]
pub(super) fn collect_items_from_poly_trait(
    krate: &rustdoc_types::Crate,
    poly_trait: &rustdoc_types::PolyTrait,
    own_crate_key: &str,
    prefix_match: bool,
    seen: &mut HashSet<String>,
    discovered: &mut Vec<ExtractedItem>,
) {
    collect_items_from_path(
        krate,
        &poly_trait.trait_,
        own_crate_key,
        prefix_match,
        seen,
        discovered,
    );
    collect_items_from_generic_param_defs(
        krate,
        &poly_trait.generic_params,
        own_crate_key,
        prefix_match,
        seen,
        discovered,
    );
}
#[instrument(skip(krate, generic_params, seen, discovered), fields(param_count = generic_params.len()))]
pub(super) fn collect_items_from_generic_param_defs(
    krate: &rustdoc_types::Crate,
    generic_params: &[rustdoc_types::GenericParamDef],
    own_crate_key: &str,
    prefix_match: bool,
    seen: &mut HashSet<String>,
    discovered: &mut Vec<ExtractedItem>,
) {
    for generic in generic_params {
        match &generic.kind {
            rustdoc_types::GenericParamDefKind::Type {
                bounds, default, ..
            } => {
                for bound in bounds {
                    collect_items_from_generic_bound(
                        krate,
                        bound,
                        own_crate_key,
                        prefix_match,
                        seen,
                        discovered,
                    );
                }
                if let Some(default) = default {
                    collect_items_from_type(
                        krate,
                        default,
                        own_crate_key,
                        prefix_match,
                        seen,
                        discovered,
                    );
                }
            }
            rustdoc_types::GenericParamDefKind::Const { type_, .. } => {
                collect_items_from_type(
                    krate,
                    type_,
                    own_crate_key,
                    prefix_match,
                    seen,
                    discovered,
                );
            }
            rustdoc_types::GenericParamDefKind::Lifetime { .. } => {}
        }
    }
}
#[instrument(skip(krate, generics, seen, discovered), fields(param_count = generics.params.len(), predicate_count = generics.where_predicates.len()))]
pub(super) fn collect_items_from_generics(
    krate: &rustdoc_types::Crate,
    generics: &rustdoc_types::Generics,
    own_crate_key: &str,
    prefix_match: bool,
    seen: &mut HashSet<String>,
    discovered: &mut Vec<ExtractedItem>,
) {
    collect_items_from_generic_param_defs(
        krate,
        &generics.params,
        own_crate_key,
        prefix_match,
        seen,
        discovered,
    );
    for predicate in &generics.where_predicates {
        collect_items_from_where_predicate(
            krate,
            predicate,
            own_crate_key,
            prefix_match,
            seen,
            discovered,
        );
    }
}
#[instrument(skip(krate, predicate, own_crate_key, prefix_match, seen, discovered))]
fn collect_items_from_where_predicate(
    krate: &rustdoc_types::Crate,
    predicate: &rustdoc_types::WherePredicate,
    own_crate_key: &str,
    prefix_match: bool,
    seen: &mut HashSet<String>,
    discovered: &mut Vec<ExtractedItem>,
) {
    match predicate {
        rustdoc_types::WherePredicate::BoundPredicate {
            type_,
            bounds,
            generic_params,
        } => {
            collect_items_from_type(krate, type_, own_crate_key, prefix_match, seen, discovered);
            for bound in bounds {
                collect_items_from_generic_bound(
                    krate,
                    bound,
                    own_crate_key,
                    prefix_match,
                    seen,
                    discovered,
                );
            }
            collect_items_from_generic_param_defs(
                krate,
                generic_params,
                own_crate_key,
                prefix_match,
                seen,
                discovered,
            );
        }
        rustdoc_types::WherePredicate::EqPredicate { lhs, rhs } => {
            collect_items_from_type(krate, lhs, own_crate_key, prefix_match, seen, discovered);
            collect_items_from_term(krate, rhs, own_crate_key, prefix_match, seen, discovered);
        }
        rustdoc_types::WherePredicate::LifetimePredicate { .. } => {}
    }
}
#[instrument(skip(ty))]
pub(super) fn type_kind_name(ty: &rustdoc_types::Type) -> &'static str {
    match ty {
        rustdoc_types::Type::ResolvedPath(_) => "ResolvedPath",
        rustdoc_types::Type::DynTrait(_) => "DynTrait",
        rustdoc_types::Type::Generic(_) => "Generic",
        rustdoc_types::Type::Primitive(_) => "Primitive",
        rustdoc_types::Type::FunctionPointer(_) => "FunctionPointer",
        rustdoc_types::Type::Tuple(_) => "Tuple",
        rustdoc_types::Type::Slice(_) => "Slice",
        rustdoc_types::Type::Array { .. } => "Array",
        rustdoc_types::Type::ImplTrait(_) => "ImplTrait",
        rustdoc_types::Type::Infer => "Infer",
        rustdoc_types::Type::RawPointer { .. } => "RawPointer",
        rustdoc_types::Type::BorrowedRef { .. } => "BorrowedRef",
        rustdoc_types::Type::QualifiedPath { .. } => "QualifiedPath",
        _ => "Other",
    }
}

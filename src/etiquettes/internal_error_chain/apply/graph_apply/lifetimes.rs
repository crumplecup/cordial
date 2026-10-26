//! Lifetime closure for a file-local private error graph.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use syn::{GenericParam, Type};
use tracing::instrument;

use crate::config::ErrorFieldStrategy;
use crate::loader::SharedParsedFile;

use super::super::Edit;
use super::super::inventory::CrateTypeIndex;
use super::types::{is_copy_shape, storage_shape, type_name};

mod collect;
mod plan;
mod provenance;

pub(super) use provenance::{anchored_parameters, constructor_origin};

type Range = (usize, usize);

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(in crate::etiquettes::internal_error_chain::apply) struct GraphLifetimeFacts {
    declarations: Vec<Declaration>,
    impls: Vec<ImplSite>,
    functions: Vec<FunctionSite>,
    type_paths: Vec<TypePathSite>,
    expr_path_blockers: BTreeSet<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Declaration {
    name: String,
    is_enum: bool,
    module_depth: usize,
    ident_end: Option<usize>,
    generics_empty: bool,
    one_lifetime: Option<String>,
    generics_range: Option<Range>,
    where_clause: bool,
    needs_borrowed: bool,
    needs_owned: bool,
    retained_lifetimes: BTreeSet<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ImplSite {
    name: Option<String>,
    module_depth: usize,
    impl_token_end: Option<usize>,
    generics_empty: bool,
    one_lifetime: Option<String>,
    generics_range: Option<Range>,
    trait_lifetimes: BTreeSet<String>,
    methods: Vec<MethodSite>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MethodSite {
    output_self: bool,
    all_inputs_typed: bool,
    generics_empty: bool,
    one_lifetime: Option<String>,
    generics_range: Option<Range>,
    ident_end: Option<usize>,
    safe_constructor_lifetimes: BTreeSet<String>,
    used_lifetimes: BTreeSet<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FunctionSite {
    return_names: BTreeSet<String>,
    safe_returns: BTreeMap<String, BTreeSet<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TypePathSite {
    name: String,
    range: Option<Range>,
    ident_end: Option<usize>,
    arguments: TypeArguments,
    qself: bool,
    path_len: usize,
    scope_lifetimes: BTreeSet<String>,
    conditional_scope: Option<ConditionalScope>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum TypeArguments {
    None,
    OneLifetime { name: String, range: Option<Range> },
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ConditionalScope {
    owner: String,
    borrowed_only: bool,
}

#[derive(derive_getters::Getters)]
pub(super) struct GraphLifetimePlan {
    lifetime: String,
    names: BTreeSet<String>,
}

impl GraphLifetimePlan {
    #[instrument(level = "trace", skip(self, facts, strategy, storage_edits))]
    pub(super) fn edits(
        &self,
        facts: &GraphLifetimeFacts,
        strategy: ErrorFieldStrategy,
        storage_edits: &[Edit],
    ) -> Option<Vec<Edit>> {
        facts.plan(self, strategy, storage_edits, false)
    }

    #[instrument(level = "trace", skip(self, facts, strategy, storage_edits))]
    pub(super) fn distributed_edits(
        &self,
        facts: &GraphLifetimeFacts,
        strategy: ErrorFieldStrategy,
        storage_edits: &[Edit],
    ) -> Option<Vec<Edit>> {
        facts.plan(self, strategy, storage_edits, true)
    }
}

#[instrument(level = "debug", skip(file, strategy, names, index))]
pub(super) fn select(
    file: &SharedParsedFile,
    strategy: ErrorFieldStrategy,
    names: &BTreeSet<String>,
    index: &CrateTypeIndex,
) -> Option<GraphLifetimePlan> {
    let facts = index.lifetime_facts(file)?;
    let sites = facts
        .declarations
        .iter()
        .filter(|site| site.module_depth == 0)
        .map(|site| (site.name.clone(), site))
        .collect();
    select_sites(strategy, names, &sites, index)
}

#[instrument(level = "debug", skip(strategy, names, owners, index))]
pub(super) fn select_across(
    strategy: ErrorFieldStrategy,
    names: &BTreeSet<String>,
    owners: &BTreeMap<String, &SharedParsedFile>,
    index: &CrateTypeIndex,
) -> Option<GraphLifetimePlan> {
    let mut sites = BTreeMap::new();
    for (name, file) in owners {
        let site = index
            .lifetime_facts(file)?
            .declarations
            .iter()
            .find(|site| site.module_depth == 0 && &site.name == name)?;
        sites.insert(name.clone(), site);
    }
    select_sites(strategy, names, &sites, index)
}

#[instrument(level = "debug", skip(strategy, names, sites, index))]
fn select_sites(
    strategy: ErrorFieldStrategy,
    names: &BTreeSet<String>,
    sites: &BTreeMap<String, &Declaration>,
    index: &CrateTypeIndex,
) -> Option<GraphLifetimePlan> {
    match strategy {
        ErrorFieldStrategy::Borrowed => {
            let changing: BTreeSet<_> = names
                .iter()
                .filter(|name| {
                    sites
                        .get(*name)
                        .is_some_and(|site| site.generics_empty && site.needs_borrowed)
                })
                .cloned()
                .collect();
            if changing.is_empty() {
                return None;
            }
            let anchors = index.anchors_for(&changing);
            if anchors.len() != 1 {
                return None;
            }
            let lifetime = anchors.into_iter().next()?;
            Some(GraphLifetimePlan {
                lifetime,
                names: changing,
            })
        }
        ErrorFieldStrategy::Owned => {
            let mut changing = BTreeSet::new();
            let mut lifetimes = BTreeSet::new();
            for name in names {
                let Some(site) = sites.get(name) else {
                    continue;
                };
                let Some(lifetime) = &site.one_lifetime else {
                    continue;
                };
                if site.where_clause
                    || (!site.is_enum && !site.needs_owned)
                    || site.retained_lifetimes.contains(lifetime)
                {
                    return None;
                }
                changing.insert(name.clone());
                lifetimes.insert(lifetime.clone());
            }
            if changing.is_empty() || lifetimes.len() != 1 {
                return None;
            }
            Some(GraphLifetimePlan {
                lifetime: lifetimes.into_iter().next()?,
                names: changing,
            })
        }
    }
}

#[instrument(level = "trace", skip(field, strategy))]
fn field_needs_lifetime(field: &syn::Field, strategy: ErrorFieldStrategy) -> bool {
    if field.ident.as_ref().is_some_and(|name| name == "file") || storage_shape(&field.ty).is_none()
    {
        return false;
    }
    match strategy {
        ErrorFieldStrategy::Borrowed => !is_copy_shape(&field.ty),
        ErrorFieldStrategy::Owned => {
            matches!(field.ty, Type::Reference(_))
                && (field.ident.is_none()
                    || field
                        .ident
                        .as_ref()
                        .is_some_and(|name| name == "kind" || name == "source")
                    || type_name(&field.ty).as_deref() == Some("str"))
        }
    }
}

#[instrument(level = "trace", skip(generics))]
fn one_lifetime(generics: &syn::Generics) -> Option<String> {
    let [GenericParam::Lifetime(parameter)] = generics.params.iter().collect::<Vec<_>>().as_slice()
    else {
        return None;
    };
    (parameter.attrs.is_empty() && parameter.bounds.is_empty())
        .then(|| parameter.lifetime.ident.to_string())
}

//! Resolved CLI type and dispatch evidence, independent of syntax backend.
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};
use tracing::instrument;

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    derive_getters::Getters,
    derive_builder::Builder,
)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub struct TypeRec {
    ident: String,
    type_path: String,
    file: PathBuf,
    #[getter(copy)]
    line: u32,
    snippet: String,
    #[getter(copy)]
    parser: bool,
    #[getter(copy)]
    subcommand: bool,
    #[getter(copy)]
    error: bool,
    #[getter(copy)]
    in_library: bool,
    fields: BTreeMap<String, Vec<String>>,
    variants: BTreeMap<String, VariantShape>,
}

impl TypeRec {
    #[instrument(level = "debug")]
    pub(crate) fn builder() -> TypeRecBuilder {
        TypeRecBuilder::default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VariantShape {
    Named(BTreeMap<String, Vec<String>>),
    Unnamed(Vec<Vec<String>>),
    Unit,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters, derive_new::new,
)]
pub struct ActRec {
    file: PathBuf,
    #[getter(copy)]
    line: u32,
    called_on: BTreeSet<String>,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters, derive_new::new,
)]
pub struct FreeFnRec {
    name: String,
    file: PathBuf,
    #[getter(copy)]
    line: u32,
    #[getter(copy)]
    in_library: bool,
    input_idents: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct LayoutCatalog {
    crate_name: String,
    types: BTreeMap<String, TypeRec>,
    acts: BTreeMap<String, ActRec>,
    free_fns: Vec<FreeFnRec>,
}

impl LayoutCatalog {
    #[instrument(level = "debug", fields(crate_name = crate_name))]
    pub(crate) fn new(crate_name: String) -> Self {
        Self {
            crate_name,
            types: BTreeMap::new(),
            acts: BTreeMap::new(),
            free_fns: Vec::new(),
        }
    }

    #[instrument(level = "debug", skip(self))]
    pub(crate) fn acts_mut(&mut self) -> &mut BTreeMap<String, ActRec> {
        &mut self.acts
    }
}

impl TypeRec {
    #[instrument(level = "trace", skip(self, fields, variants, error))]
    pub(super) fn merge(
        &mut self,
        parser: bool,
        subcommand: bool,
        error: bool,
        fields: BTreeMap<String, Vec<String>>,
        variants: BTreeMap<String, VariantShape>,
        in_library: bool,
    ) {
        self.parser |= parser;
        self.subcommand |= subcommand;
        self.error |= error;
        self.fields.extend(fields);
        self.variants.extend(variants);
        self.in_library |= in_library;
    }
    #[instrument(level = "trace", skip(self))]
    pub(super) fn mark_error(&mut self) {
        self.error = true;
    }
}
impl LayoutCatalog {
    #[instrument(level = "trace", skip(self, item))]
    pub(super) fn insert_type(&mut self, item: TypeRec) {
        self.types.insert(item.ident.clone(), item);
    }
    #[instrument(level = "trace", skip(self))]
    pub(super) fn types_mut(&mut self) -> &mut BTreeMap<String, TypeRec> {
        &mut self.types
    }
    #[instrument(level = "trace", skip(self, item))]
    pub(super) fn add_free_fn(&mut self, item: FreeFnRec) {
        self.free_fns.push(item);
    }
}

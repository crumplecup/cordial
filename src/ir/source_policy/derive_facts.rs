//! Backend-independent declaration and method shapes for derive policy.
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

/// Ordered derive declarations in one crate-relative source file.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_new::new, derive_getters::Getters,
)]
pub struct DeriveFileFacts {
    file: PathBuf,
    module_prefix: Vec<String>,
    items: Vec<DeriveItemFacts>,
}

/// Declaration hierarchy, retaining source order and module boundaries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeriveItemFacts {
    /// A struct declaration.
    Struct(DeriveStructFacts),
    /// An inherent or trait implementation.
    Impl(DeriveImplFacts),
    /// An inline module.
    Module(DeriveModuleFacts),
}

/// Inline module declarations and gates, before policy exclusions.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_new::new, derive_getters::Getters,
)]
pub struct DeriveModuleFacts {
    name: String,
    #[getter(copy)]
    test_only: bool,
    #[getter(copy)]
    creusot_only: bool,
    items: Vec<DeriveItemFacts>,
}

/// Fields, derive attributes, and visibility evidence for one struct.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    derive_builder::Builder,
    derive_getters::Getters,
)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub struct DeriveStructFacts {
    name: String,
    #[getter(copy)]
    line: u32,
    derives: Vec<String>,
    fields: BTreeMap<String, DeriveFieldFacts>,
    exposed_fields: Vec<String>,
    generated_builders: Vec<String>,
    #[getter(copy)]
    clap_schema: bool,
    #[getter(copy)]
    creusot_only: bool,
}

impl DeriveStructFacts {
    /// Start a builder for this value.
    #[tracing::instrument(level = "debug")]
    pub(crate) fn builder() -> DeriveStructFactsBuilder {
        DeriveStructFactsBuilder::default()
    }
}

/// Field wrapper and visibility facts used by getter policy.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_new::new, derive_getters::Getters,
)]
pub struct DeriveFieldFacts {
    #[getter(copy)]
    is_public: bool,
    #[getter(copy)]
    is_option: bool,
    boxed_inner_type: Option<String>,
}

/// Trait membership and method evidence for an implementation.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_new::new, derive_getters::Getters,
)]
pub struct DeriveImplFacts {
    self_type: String,
    #[getter(copy)]
    line: u32,
    #[getter(copy)]
    trait_impl: bool,
    #[getter(copy)]
    error_impl: bool,
    methods: Vec<DeriveMethodFacts>,
}

/// Signature flags and supported body shapes for one method.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_new::new, derive_getters::Getters,
)]
pub struct DeriveMethodFacts {
    name: String,
    #[getter(copy)]
    line: u32,
    flags: DeriveMethodFlags,
    field_read: Option<(String, DeriveFieldRead)>,
    #[getter(copy)]
    setter: Option<DeriveSetterShape>,
    return_reference: Option<String>,
    /// Arity only when a returned struct literal forwards every parameter.
    #[getter(copy)]
    constructor: Option<usize>,
}

/// Receiver, return, and attribute properties independent of policy.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_new::new, derive_getters::Getters,
)]
pub struct DeriveMethodFlags {
    #[getter(copy)]
    is_const: bool,
    #[getter(copy)]
    track_caller: bool,
    #[getter(copy)]
    immutable_reference: bool,
    #[getter(copy)]
    mutable_receiver: bool,
    #[getter(copy)]
    consumes_self: bool,
    #[getter(copy)]
    fluent_setter: bool,
}

/// A body reading one field, without additional work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeriveFieldRead {
    /// An explicit reference to a field.
    Direct,
    /// A bare field value.
    DirectOwned,
    /// A cloned or owned field value.
    Clone,
    /// A field forwarded through as_str.
    AsStr,
    /// A field forwarded through as_ref.
    AsRef,
}

/// Assignment forms supported by derive_setters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeriveSetterShape {
    /// A direct parameter assignment.
    Assign,
    /// An ownership conversion before assignment.
    Into,
    /// A parameter wrapped in Some.
    StripOption,
    /// An ownership conversion wrapped in Some.
    StripOptionInto,
}

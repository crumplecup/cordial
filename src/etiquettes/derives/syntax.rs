//! syn syntax predicates used by the derives scanner.

mod attrs;
mod constructor;
mod exprs;
mod receivers;
mod setters;
mod types;

pub(super) use attrs::{
    derive_builder_names, error_impl_target, has_derive, has_track_caller, is_cfg_creusot,
    is_cfg_test, is_clap_schema,
};
pub(super) use constructor::{body_is_struct_literal, constructor_fields_match_params};
pub(super) use exprs::{FieldRead, classify_field_read};
pub(super) use receivers::{
    constructor_arg_count, consumes_self, field_is_exposed, is_fluent_setter,
    receiver_is_immutable_reference, receiver_is_mutable,
};
pub(super) use setters::classify_setter_body;
pub(super) use types::type_label;

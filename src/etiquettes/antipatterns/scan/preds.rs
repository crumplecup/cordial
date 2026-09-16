//! Type and pattern predicates for antipattern rules.

mod cfg_siblings;
mod errors;
mod labels;
mod static_refs;
mod unused_args;

pub(super) use cfg_siblings::{
    cfg_sibling_real_param_names_in_impl_items, cfg_sibling_real_param_names_in_items,
    has_proc_macro_abi_attr, is_creusot_opaque_logic_stub,
};
pub(super) use errors::{
    box_dyn_error_snippet, box_dyn_error_trait_object, is_stringish_error_type, result_error_type,
    result_string_error_snippet,
};
pub(crate) use labels::truncate_snippet;
pub(super) use labels::type_label;
pub(super) use static_refs::{
    static_ref_field_snippet, type_contains_disallowed_static_ref, type_is_location_capture,
};
pub(super) use unused_args::unused_argument_bindings;

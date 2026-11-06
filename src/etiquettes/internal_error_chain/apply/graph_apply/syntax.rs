//! Shared canonical-tree contracts for graph collection.
mod anchors;
mod paths;

pub(super) use anchors::{origin, parameters};
pub(super) use paths::{
    bare_path, boxed_call, is_source_method, method, names, record_name, self_field, type_name,
};

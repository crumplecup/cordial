//! Classify a function into [`FunctionRole`] and [`FunctionComplexity`].

use crate::config::ModularityThresholds;

use super::types::{FunctionComplexity, FunctionKind, FunctionRole};

use tracing::instrument;

#[instrument(level = "debug", skip(receiver, kind, peek))]
pub(crate) fn classify_role(
    ident: &str,
    receiver: &ReceiverShape,
    kind: FunctionKind,
    peek: &BodyPeek,
    returns_self: bool,
    returns_bool: bool,
    returns_result: bool,
) -> FunctionRole {
    if is_constructor(ident, returns_self) {
        return FunctionRole::Constructor;
    }
    if is_getter(ident, receiver, peek, returns_result) {
        return FunctionRole::Getter;
    }
    if is_setter(ident, receiver) {
        return FunctionRole::Setter;
    }
    if is_predicate(ident, returns_bool) {
        return FunctionRole::Predicate;
    }
    if has_prefix(ident, &["scan_", "walk_", "visit_", "collect_"]) {
        return FunctionRole::Scan;
    }
    if ident == "ensure_dirs" || has_prefix(ident, &["load_", "read_", "write_", "fetch_"]) {
        return FunctionRole::Io;
    }
    if is_render(ident) {
        return FunctionRole::Render;
    }
    if kind == FunctionKind::TraitImplMethod {
        return FunctionRole::TraitSurface;
    }
    if is_entry(ident) {
        return FunctionRole::Entry;
    }
    FunctionRole::Other
}

#[instrument(level = "trace", ret)]
fn is_constructor(ident: &str, returns_self: bool) -> bool {
    matches!(ident, "new" | "try_new" | "default")
        || ((ident == "from" || ident.starts_with("from_")) && returns_self)
}

#[derive(Debug, derive_new::new)]
pub(crate) struct ReceiverShape {
    immutable_reference: bool,
    mutable_value: bool,
    input_count: usize,
}

#[instrument(level = "trace", skip(receiver, peek))]
fn is_getter(ident: &str, receiver: &ReceiverShape, peek: &BodyPeek, returns_result: bool) -> bool {
    if returns_result || !receiver.immutable_reference {
        return false;
    }
    if getter_name(ident) {
        return true;
    }
    peek.stmts <= 2 && !peek.has_branch
}

#[instrument(level = "debug")]
fn getter_name(ident: &str) -> bool {
    ident.starts_with("as_")
        || ident.starts_with("to_")
        || ident == "id"
        || ident.ends_with("_dir")
        || ident.ends_with("_path")
        || ident.ends_with("_name")
}

#[instrument(level = "trace", ret)]
fn is_render(ident: &str) -> bool {
    ident.starts_with("render_")
        || ident.ends_with("_summary")
        || ident.ends_with("_checklist")
        || ident.ends_with("_csv")
}

#[instrument(level = "trace", ret)]
fn is_entry(ident: &str) -> bool {
    ident == "main" || ident == "run" || ident.starts_with("run_")
}

#[instrument(level = "trace", skip(receiver))]
fn is_setter(ident: &str, receiver: &ReceiverShape) -> bool {
    ident.starts_with("set_")
        || ident.starts_with("with_")
        || (receiver.mutable_value && receiver.input_count >= 2)
}

#[instrument(level = "trace", ret)]
fn is_predicate(ident: &str, returns_bool: bool) -> bool {
    returns_bool
        && (has_prefix(ident, &["is_", "has_", "can_", "contains_"]) || ident.starts_with("eq_"))
}

#[instrument(level = "trace", ret)]
fn has_prefix(ident: &str, prefixes: &[&str]) -> bool {
    prefixes.iter().any(|prefix| ident.starts_with(prefix))
}

#[instrument(level = "debug", skip(peek))]
pub(crate) fn classify_complexity(
    body_lines: u32,
    returns_result: bool,
    peek: &BodyPeek,
) -> FunctionComplexity {
    let hotspot_floor = ModularityThresholds::default().function_inventory_min_lines();
    if body_lines >= hotspot_floor {
        return FunctionComplexity::Hotspot;
    }
    if returns_result {
        return FunctionComplexity::Fallible;
    }
    if peek.has_branch {
        return FunctionComplexity::Branchy;
    }
    if peek.stmts <= 2 {
        return FunctionComplexity::Trivial;
    }
    FunctionComplexity::Linear
}

#[derive(Debug, Default, derive_new::new, derive_getters::Getters)]
pub(crate) struct BodyPeek {
    #[getter(skip)]
    stmts: usize,
    #[getter(skip)]
    has_branch: bool,
    #[getter(copy)]
    has_error_path_event: bool,
}

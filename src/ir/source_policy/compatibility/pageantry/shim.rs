//! Proc-macro entry points in barrel files: allowed as shims, flagged as logic.

use tracing::instrument;

/// The proc-macro attribute on `attrs`, if any.
///
/// rustc requires `#[proc_macro]`, `#[proc_macro_derive]`, and
/// `#[proc_macro_attribute]` functions to sit at the root of a
/// `proc-macro = true` crate. They cannot move to a sibling file and be
/// re-exported, so the barrel rule asks them to delegate instead.
#[instrument(level = "trace", skip(attrs), ret)]
pub(super) fn proc_macro_entry_attr(attrs: &[syn::Attribute]) -> Option<&'static str> {
    attrs.iter().find_map(|attr| {
        let path = attr.path();
        if path.is_ident("proc_macro") {
            Some("proc_macro")
        } else if path.is_ident("proc_macro_derive") {
            Some("proc_macro_derive")
        } else if path.is_ident("proc_macro_attribute") {
            Some("proc_macro_attribute")
        } else {
            None
        }
    })
}

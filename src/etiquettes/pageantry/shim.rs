//! Proc-macro entry points in barrel files: allowed as shims, flagged as logic.

use tracing::instrument;

use super::types::PageantryRuleId;

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

/// Lines strictly between the braces of a function body.
#[instrument(level = "trace", skip(item), ret)]
fn body_lines(item: &syn::ItemFn) -> usize {
    let open = item.block.brace_token.span.open().start().line;
    let close = item.block.brace_token.span.close().start().line;
    close.saturating_sub(open).saturating_sub(1)
}

/// A shim-size finding for a proc-macro entry point, when its body is too long.
///
/// Returns `None` for a function that is not an entry point or that
/// stays within `max_shim_lines`.
#[instrument(level = "trace", skip(item), ret)]
pub(super) fn shim_overflow(
    item: &syn::ItemFn,
    max_shim_lines: usize,
) -> Option<(PageantryRuleId, u32, String)> {
    let attr = proc_macro_entry_attr(&item.attrs)?;
    let lines = body_lines(item);
    if lines <= max_shim_lines {
        return None;
    }
    Some((
        PageantryRuleId::BarrelShim001,
        item.sig.ident.span().start().line as u32,
        format!(
            "#[{attr}] fn {} has a {lines}-line body; shims may have {max_shim_lines} — move the logic to a named file and delegate",
            item.sig.ident
        ),
    ))
}

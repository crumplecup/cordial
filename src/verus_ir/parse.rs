//! Find and parse every `verus! { .. }` block in a crate with `verus_syn`
//! -- the same real parser `verus_builtin_macros` itself uses, so this
//! doesn't hand-roll any part of Verus's own grammar (`requires`/
//! `ensures`/`decreases`/spec modes/the `@` view operator/quantifiers,
//! none of which plain `syn` can parse -- see `crate::etiquettes::
//! panics::verus_recover`'s own doc comment for the narrower, syn-only
//! fallback this supersedes for any crate `verus_syn` can actually
//! parse).
//!
//! Two-step parse, confirmed against `verus_builtin_macros::syntax::
//! rewrite_items`'s own approach (and already proven working in
//! `amenable_core::verus_carrier`, this exact codebase's own prior art):
//! plain `syn::parse_file` finds the `verus! { .. }` macro invocation
//! itself (an ordinary, opaque-bodied macro as far as `syn` is
//! concerned), then `verus_syn::parse2` parses *just* the invocation's
//! own token stream -- `verus_syn` expects to parse that content
//! directly, not a whole ordinary Rust file with an embedded macro call.

#[cfg(feature = "visibility")]
mod visibility;

use std::path::{Path, PathBuf};

use verus_syn::parse::{Parse, ParseStream};

use tracing::instrument;

/// Mirrors `verus_builtin_macros::syntax::Items` (private to that
/// crate): a bare sequence of items, exactly what sits inside a
/// `verus! { .. }` macro body.
struct Items {
    items: Vec<verus_syn::Item>,
}

impl Parse for Items {
    #[instrument(level = "trace", skip(input))]
    fn parse(input: ParseStream) -> verus_syn::parse::Result<Items> {
        let mut items = Vec::new();
        while !input.is_empty() {
            items.push(input.parse()?);
        }
        Ok(Items { items })
    }
}

/// One `verus! { .. }` block found in one source file: its real parsed
/// items, plus enough context (the source file, crate-relative module
/// path, and whether the macro invocation itself sits inside a
/// `#[cfg(test)]` module) to attribute facts extracted from it
/// correctly -- matching `crate::etiquettes::panics::scan`'s own
/// `cfg_test` tracking, since a real consumer needs the same test-vs-
/// library routing this codebase's panics policy already uses.
pub(super) struct VerusBlock {
    file: PathBuf,
    module_path: String,
    cfg_test: bool,
    items: Vec<verus_syn::Item>,
}

impl VerusBlock {
    #[instrument(level = "debug", skip(self))]
    pub(super) fn into_parts(self) -> (PathBuf, String, bool, Vec<verus_syn::Item>) {
        (self.file, self.module_path, self.cfg_test, self.items)
    }
}

/// Parse a loader-issued macro payload, preserving original line coordinates.
#[tracing::instrument(level = "debug", skip(payload, file))]
pub(super) fn block_in_payload(
    payload: &str,
    file: &Path,
    module_path: &str,
    cfg_test: bool,
) -> Option<VerusBlock> {
    let parsed = verus_syn::parse_str::<Items>(payload).ok()?;
    Some(VerusBlock {
        file: file.to_path_buf(),
        module_path: module_path.to_owned(),
        cfg_test,
        items: parsed.items,
    })
}

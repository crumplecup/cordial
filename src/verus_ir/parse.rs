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

mod finder;
pub(super) use finder::blocks_in_syntax;
#[cfg(feature = "visibility")]
mod visibility;

use std::path::{Path, PathBuf};

use verus_syn::parse::{Parse, ParseStream};

use crate::loader::{
    SourceLoadView, module_path_from_src_file, path_has_fixtures, quality_scan_trees,
};

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

/// Find every `verus! { .. }` block under `crate_root`'s `src`/`tests`
/// trees (excluding `fixtures/`, matching every other quality scanner's
/// own scope) and parse each with real `verus_syn`. A block that fails
/// to parse (a real syntax error, or a `verus_syn` version mismatch
/// against the toolchain that generated the source) is silently
/// skipped, not reported as an error -- best-effort inventory, the same
/// posture `verus_carrier::find_fn` already takes for the identical
/// reason (a partial, real answer beats none).
#[instrument(level = "debug", err(level = "warn"))]
pub(super) fn collect_verus_blocks(
    crate_root: &Path,
) -> crate::error::CordialResult<Vec<VerusBlock>> {
    let source = SourceLoadView::for_standalone_scan();
    collect_verus_blocks_impl(crate_root, &source)
}

#[instrument(level = "debug", skip(source), err(level = "warn"))]
fn collect_verus_blocks_impl(
    crate_root: &Path,
    source: &SourceLoadView,
) -> crate::error::CordialResult<Vec<VerusBlock>> {
    let mut blocks = Vec::new();
    for src_root in quality_scan_trees(crate_root) {
        if !src_root.is_dir() {
            continue;
        }
        for entry in walkdir::WalkDir::new(&src_root)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
        {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "rs") || path_has_fixtures(path, crate_root)
            {
                continue;
            }
            let module_path = module_path_from_src_file(&src_root, path).join("::");
            let parsed = source.parsed_file(path, "verus-ir")?;
            blocks.extend(blocks_in_syntax(parsed.syntax(), path, &module_path));
        }
    }
    Ok(blocks)
}

/// Find and parse every `verus! { .. }` block in one already-read
/// source string -- the shared core `collect_verus_blocks` uses per
/// file, and a direct entry point for testing against one file's
/// content without needing a real crate tree on disk.
#[instrument(level = "debug", skip(source, file))]
pub(super) fn blocks_in_source(source: &str, file: &Path, module_path: &str) -> Vec<VerusBlock> {
    let Ok(syntax) = syn::parse_file(source) else {
        return Vec::new();
    };
    blocks_in_syntax(&syntax, file, module_path)
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

//! Real, structural facts about `verus! { .. }` code, parsed with
//! `verus_syn` -- the same parser `verus_builtin_macros` itself uses,
//! not best-effort token recovery.
//!
//! **What.** Finds every `verus! { .. }` block in a crate and parses it
//! into real `verus_syn` items, extracting per-function facts: declared
//! mode (`spec`/`proof`/`exec`), publish visibility (`open`/`closed`/
//! `uninterp`), `requires`/`ensures`/`decreases` clauses (rendered back
//! to text), and real soundness-relevant signals -- `assume(..)`/
//! `admit()` calls in the body, `#[verifier::external_body]`.
//!
//! **Why.** `amenable_verus` (and any Verus-heavy crate like it) is
//! otherwise opaque to every other scanner in this tool: `syn::visit::
//! Visit` never descends into a macro's own token stream, and Verus's
//! own grammar extensions (`requires`/`ensures`/the `@` view operator/
//! quantifiers) aren't parseable by plain `syn` even if something did.
//! This is foundational infrastructure other analysis builds on -- it
//! isn't part of `quality` itself (no CSV/checklist/reporter here), but
//! [`VerusCrateIr::is_documented_pattern_projection_enum`] is a real
//! consumer: `etiquettes::verus_warnings` uses it to recognize (and
//! suppress) a "missing documentation for a method" warning about a
//! data-carrying enum's own compiler-synthesized, undocumentable
//! pattern-projection accessor, matching this workspace's own
//! [`crate::ir`] separation between "the parsed structure" and "the
//! rule that flags something about it."
//!
//! **How to use.** Feature `verus_ir`. [`scan_crate_verus_ir`] returns a
//! crate's [`VerusCrateIr`]; best-effort throughout (a block or function
//! that fails to parse is silently skipped, not reported as an error --
//! a partial, real answer beats none, the same posture `amenable_core::
//! verus_carrier`'s own discovery already takes).

mod facts;
mod parse;
mod types;

pub use types::{
    VerusCrateIr, VerusEnumFacts, VerusEnumVariantFacts, VerusFnFacts, VerusFnMode, VerusPanicKind,
    VerusPanicSite, VerusPublish,
};

use std::path::Path;

use tracing::instrument;

/// Parse every `verus! { .. }` block under `crate_root` and extract real
/// per-function facts from each. See this module's own doc comment for
/// scope and the best-effort posture.
#[instrument(level = "debug", err(level = "warn"))]
pub fn scan_crate_verus_ir(crate_root: &Path) -> crate::error::CordialResult<VerusCrateIr> {
    let blocks = parse::collect_verus_blocks(crate_root)?;
    Ok(facts::build_crate_ir(blocks))
}

/// Parse every `verus! { .. }` block in one already-read source string
/// and extract real per-function facts from each -- a direct entry
/// point for testing against one file's content without a real crate
/// tree on disk, matching [`scan_crate_verus_ir`]'s own best-effort
/// posture.
#[instrument(level = "debug", skip(source))]
pub fn scan_verus_rust_source(source: &str, file: &Path, module_path: &str) -> VerusCrateIr {
    let blocks = parse::blocks_in_source(source, file, module_path);
    facts::build_crate_ir(blocks)
}

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
//! **How to use.** Feature `verus_ir`. [`crate::SourceVerus::facts`] returns a
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

/// Context-free facts and visibility counts from one memoized dialect parse.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters)]
pub(crate) struct VerusPayloadFacts {
    facts: VerusCrateIr,
    #[cfg(feature = "visibility")]
    #[getter(copy)]
    visible_names: (usize, usize),
}

/// Extract a loader-issued dialect payload once, independent of consumer context.
#[tracing::instrument(level = "debug", skip(payload))]
pub(crate) fn payload_facts(payload: &str) -> Option<VerusPayloadFacts> {
    let block = parse::block_in_payload(payload, Path::new(""), "", false)?;
    #[cfg(feature = "visibility")]
    let visible_names = block.visible_names();
    let facts = facts::collect_crate_ir(vec![block]).ok()?;
    Some(VerusPayloadFacts {
        facts,
        #[cfg(feature = "visibility")]
        visible_names,
    })
}

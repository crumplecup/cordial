use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::objects::FileSpan;

use tracing::instrument;

/// A function's real declared execution mode -- `spec`, `proof`, or
/// `exec` -- mirroring `verus_syn::verus::FnMode`'s own variants as an
/// owned, `Serialize`-able value (verus_syn's own AST types borrow
/// tokens/spans, awkward to hold onto past the parse that produced
/// them).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VerusFnMode {
    /// `spec fn` -- a pure, unchecked-by-default logical definition.
    Spec,
    /// `spec(checked) fn` -- a spec fn Verus additionally checks for
    /// well-formedness (e.g. recommends clauses).
    SpecChecked,
    /// `proof fn` -- ghost code checked by the SMT solver, erased before
    /// compilation.
    Proof,
    /// `proof fn` declared `#[verifier::axiom]` -- an assumed, not
    /// proven, ghost fact. Real soundness signal: every real proof this
    /// fn backs rests on this being true, not on anything Verus itself
    /// checked.
    ProofAxiom,
    /// `exec fn` (or unmarked) -- ordinary compiled, executable code,
    /// Verus's default mode.
    Exec,
    /// No mode keyword present at all.
    Default,
}

/// A function's real declared visibility to Verus's own prover --
/// mirroring `verus_syn::verus::Publish`'s own variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VerusPublish {
    /// `closed spec fn` -- the body is hidden from callers' own proofs
    /// (an abstraction boundary), even though the fn itself is spec mode.
    Closed,
    /// `open spec fn` -- the body is visible to and usable by callers'
    /// own proofs.
    Open,
    /// `open(crate)`/`open(in some::path)` -- visible only within the
    /// named scope.
    OpenRestricted,
    /// `uninterp spec fn` -- declared with no body at all, real
    /// soundness signal: nothing backs this fn's meaning except the
    /// `requires`/`ensures` a caller chooses to trust.
    Uninterp,
    /// No publish keyword present -- Verus's own default for the
    /// declared mode.
    Default,
}

/// Category of abort site found inside a `verus! { .. }` function body --
/// the same five categories `crate::etiquettes::panics` tracks, found
/// here via a real, complete parse instead of the best-effort token
/// recovery `panics::verus_recover` falls back to for a body real
/// `verus_syn` can't make sense of either (there is no such body for
/// `verus_syn` in practice -- it understands the whole grammar, not just
/// ordinary-looking fragments of it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VerusPanicKind {
    /// `panic!`.
    Panic,
    /// `unreachable!`.
    Unreachable,
    /// `.expect(...)`.
    Expect,
    /// `.unwrap()`.
    Unwrap,
    /// `compile_error!`.
    CompileError,
}

/// One abort site found inside a function's body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerusPanicSite {
    /// Abort kind (`panic!`, `unwrap`, …).
    pub kind: VerusPanicKind,
    /// Source line number (1-based), when known.
    pub line: u32,
    /// Source snippet captured at the site.
    pub snippet: String,
    /// True when this site sits in a `match` arm gated
    /// `#[cfg(not(verus_keep_ghost))]` whose sibling arm -- same
    /// pattern, gated `#[cfg(verus_keep_ghost)]` -- calls `unreached()`
    /// instead. That sibling only compiles under the real `verus`
    /// toolchain, where the branch is proven impossible by the SMT
    /// solver (typically backed by a `requires` clause on the enclosing
    /// function); this site is the same branch's ordinary-rustc
    /// fallback, needed because `unreached()` requires `vstd`. A real,
    /// raw structural fact -- whether a consumer treats it as exempt
    /// from a panic-inventory policy is that consumer's own call, not
    /// this parse's.
    pub proven_unreachable_by_ghost_sibling: bool,
}

/// Real facts extracted from one `fn`/`spec fn`/`proof fn` found inside a
/// `verus! { .. }` block, via `verus_syn`'s own real Verus-aware parser
/// -- not best-effort token recovery (see `crate::etiquettes::panics::
/// verus_recover` for that narrower, syn-only fallback). Everything
/// here reflects genuine Verus AST structure: `requires`/`ensures`/
/// `decreases` are the fn's own real specification clauses, rendered
/// back to text; `uses_assume`/`uses_admit`/`is_external_body` are real
/// soundness-relevant signals -- code paths where a claim is trusted
/// rather than checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerusFnFacts {
    /// The function's own name.
    pub name: String,
    /// Crate-relative module path (`rust_std::try_from_int_error_carrier`).
    pub module_path: String,
    /// Where this function is declared.
    pub span: FileSpan,
    /// Whether the enclosing `verus! { .. }` invocation sits inside a
    /// `#[cfg(test)]` module -- matching `panics::scan`'s own tracking,
    /// so a real consumer can apply the same test-vs-library routing
    /// policy.
    pub cfg_test: bool,
    /// The function's own declared mode (`spec`/`proof`/`exec`/...).
    pub mode: VerusFnMode,
    /// The function's own declared publish visibility (spec fns only;
    /// `VerusPublish::Default` for proof/exec fns).
    pub publish: VerusPublish,
    /// Every `requires` clause, rendered back to text, in declared order.
    pub requires: Vec<String>,
    /// Every `ensures` clause, rendered back to text, in declared order.
    pub ensures: Vec<String>,
    /// The `decreases` clause, if any, rendered back to text -- present
    /// only on recursive spec/proof fns that need a termination measure.
    pub decreases: Option<String>,
    /// Whether the body calls `assume(..)` anywhere -- a real, local
    /// soundness escape hatch: the enclosed condition is trusted, not
    /// proven, from that point on.
    pub uses_assume: bool,
    /// Whether the body calls `admit()` anywhere -- discharges the
    /// entire remaining proof obligation unconditionally, the strongest
    /// local soundness escape hatch Verus has.
    pub uses_admit: bool,
    /// Whether the function carries `#[verifier::external_body]` --
    /// Verus never checks this body against its own signature at all;
    /// the `ensures` clause is trusted based on the (unverified) exec
    /// code alone.
    pub is_external_body: bool,
    /// Every `panic!`/`unreachable!`/`.expect(..)`/`.unwrap()` site found
    /// in the body -- the direct completion of this module's own
    /// motivating gap (`panics::verus_recover`'s best-effort recovery
    /// found 7 of the 13 real sites known to exist in `amenable_verus`;
    /// a real parse finds all of them).
    pub panic_sites: Vec<VerusPanicSite>,
    /// Every `requires`/`ensures`-mode `tracked` parameter's own name --
    /// a parameter Verus threads through as ghost-but-linear state
    /// rather than an ordinary value, real signal for what this
    /// function's own proof obligation actually depends on carrying.
    pub tracked_params: Vec<String>,
    /// Every `recommends` clause, rendered back to text -- a
    /// well-formedness condition Verus checks (and reports separately
    /// from `requires` failures) but doesn't require the caller to
    /// discharge; distinguishing "recommended" from "required" callers
    /// is itself a real proof-design choice worth being able to see.
    pub recommends: Vec<String>,
    /// Whether the function is declared `broadcast` -- a lemma Verus
    /// applies automatically to every proof in scope (via `use`) rather
    /// than one a caller must invoke by name; real signal for how much
    /// of a codebase's total proof burden one function actually
    /// contributes to, invisibly.
    pub is_broadcast: bool,
    /// The bare name of every function/method this body calls -- a raw,
    /// local fact (what this one function's own body contains), not a
    /// crate-wide reachability judgment; a consumer wanting "is this
    /// function ever called by another local function" builds that from
    /// every function's own `calls` list, the same two-layer split
    /// `panics::kani_reach` already uses for the analogous Kani
    /// question.
    pub calls: Vec<String>,
}

impl VerusFnFacts {
    /// Whether this function rests on any real, locally-visible
    /// soundness escape hatch (`assume`/`admit`/`external_body`/
    /// `uninterp`/an axiom-mode proof) -- the single boolean a first
    /// pass over a real proof corpus would want to filter on.
    #[instrument(level = "trace", skip(self), ret)]
    pub fn is_trusted_not_proven(&self) -> bool {
        self.uses_assume
            || self.uses_admit
            || self.is_external_body
            || matches!(self.publish, VerusPublish::Uninterp)
            || matches!(self.mode, VerusFnMode::ProofAxiom)
    }
}

/// One variant of an `enum` found inside a `verus! { .. }` block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerusEnumVariantFacts {
    /// The variant's own name.
    pub name: String,
    /// Whether the variant carries data (a tuple or named-field variant)
    /// as opposed to being a bare unit variant.
    pub carries_data: bool,
    /// Whether the variant carries a doc comment (`///` or `#[doc = ..]`).
    pub has_doc: bool,
}

/// Real facts extracted from one `enum` found inside a `verus! { .. }`
/// block, via `verus_syn`'s own real Verus-aware parser. Exists for one
/// specific, confirmed reason: Verus auto-synthesizes a hidden
/// field-projection accessor method per data field on a data-carrying
/// enum variant, to support its own `result->Variant_N` pattern-
/// projection syntax -- that accessor is never a literal AST node the
/// source declares, so a real `verus` compile reports a "missing
/// documentation for a method" warning *at the enum's own declaration
/// line* with nothing real to attach a doc comment to. Confirmed against
/// a real `verus` invocation, not assumed: neither a doc comment on the
/// variant nor on the individual data field cleared the warning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerusEnumFacts {
    /// The enum's own name.
    pub name: String,
    /// Crate-relative module path.
    pub module_path: String,
    /// Where this enum is declared.
    pub span: FileSpan,
    /// Whether the enclosing `verus! { .. }` invocation sits inside a
    /// `#[cfg(test)]` module.
    pub cfg_test: bool,
    /// Whether the enum itself carries a doc comment.
    pub has_doc: bool,
    /// Every variant, in declared order.
    pub variants: Vec<VerusEnumVariantFacts>,
}

impl VerusEnumFacts {
    /// Whether Verus will synthesize at least one hidden field-projection
    /// accessor method for this enum -- true whenever any variant
    /// carries data. See this type's own doc comment for why that
    /// matters.
    #[instrument(level = "trace", skip(self), ret)]
    pub fn synthesizes_pattern_projection_accessors(&self) -> bool {
        self.variants.iter().any(|variant| variant.carries_data)
    }

    /// Whether every human-writable doc site on this enum (the enum
    /// itself, and every data-carrying variant) is already documented --
    /// the real, complete answer for why a "missing documentation for a
    /// method" warning about this enum's own synthesized accessor has
    /// nothing left here for a human to fix. A genuinely undocumented
    /// enum or variant still leaves this `false`, so the warning still
    /// flags normally.
    #[instrument(level = "trace", skip(self), ret)]
    pub fn fully_documented(&self) -> bool {
        self.has_doc
            && self
                .variants
                .iter()
                .all(|variant| !variant.carries_data || variant.has_doc)
    }
}

/// Every `VerusFnFacts`/[`VerusEnumFacts`] recovered from one crate's
/// real `verus! { .. }` blocks.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VerusCrateIr {
    /// Verus functions inventoried in this crate.
    pub functions: Vec<VerusFnFacts>,
    /// Verus enums inventoried in this crate.
    pub enums: Vec<VerusEnumFacts>,
}

impl VerusCrateIr {
    /// Every function resting on a real, local soundness escape hatch --
    /// see [`VerusFnFacts::is_trusted_not_proven`].
    #[instrument(level = "debug", skip(self))]
    pub fn trusted_not_proven(&self) -> impl Iterator<Item = &VerusFnFacts> {
        self.functions.iter().filter(|f| f.is_trusted_not_proven())
    }

    /// Every `(function, panic site)` pair across the whole crate -- the
    /// complete replacement for `panics::verus_recover`'s best-effort
    /// 7-of-13 recovery, once a consumer wires this in.
    #[instrument(level = "debug", skip(self))]
    pub fn panic_sites(&self) -> impl Iterator<Item = (&VerusFnFacts, &VerusPanicSite)> {
        self.functions
            .iter()
            .flat_map(|f| f.panic_sites.iter().map(move |site| (f, site)))
    }

    /// Every function declared `broadcast` -- see
    /// [`VerusFnFacts::is_broadcast`].
    #[instrument(level = "debug", skip(self))]
    pub fn broadcasts(&self) -> impl Iterator<Item = &VerusFnFacts> {
        self.functions.iter().filter(|f| f.is_broadcast)
    }

    /// Whether `line` in `file` is a fully-documented, data-carrying
    /// enum's own declaration -- the real signal `verus_warnings` uses
    /// to recognize (and suppress) a "missing documentation for a
    /// method" warning about that enum's synthesized, undocumentable
    /// pattern-projection accessor. See [`VerusEnumFacts::
    /// synthesizes_pattern_projection_accessors`]/[`VerusEnumFacts::
    /// fully_documented`].
    #[instrument(level = "debug", skip(self), ret)]
    pub fn is_documented_pattern_projection_enum(&self, file: &Path, line: u32) -> bool {
        self.enums.iter().any(|item| {
            item.span.file.as_path() == file
                && item.span.line == line
                && item.synthesizes_pattern_projection_accessors()
                && item.fully_documented()
        })
    }
}

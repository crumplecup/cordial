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

/// Every `VerusFnFacts` recovered from one crate's real `verus! { .. }`
/// blocks.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VerusCrateIr {
    pub functions: Vec<VerusFnFacts>,
}

impl VerusCrateIr {
    /// Every function resting on a real, local soundness escape hatch --
    /// see [`VerusFnFacts::is_trusted_not_proven`].
    #[instrument(level = "debug", skip(self))]
    pub fn trusted_not_proven(&self) -> impl Iterator<Item = &VerusFnFacts> {
        self.functions.iter().filter(|f| f.is_trusted_not_proven())
    }
}

# `proof_patterns` etiquette

`panics` closed `verus_ir`'s motivating gap (real panic-site detection
inside `verus! { .. }` blocks). `verus_ir` carries more real, local
soundness signal than that: which functions are *trusted* rather than
*proven* (`assume`/`admit`/`external_body`/`uninterp`/`axiom`), and which
lemmas apply themselves to every proof in scope invisibly (`broadcast`).
This etiquette surfaces that signal the same way `panics` surfaces abort
sites — one open finding per site, until someone reads and dispositions
it.

Complements [verus_warnings](verus-warnings-etiquette.md) (the *other*
compiler's diagnostics) and [panics](../../src/etiquettes/panics/mod.rs)
(abort sites, including inside `verus!` blocks). This etiquette is about
what a `verus!` function's own signature and body say about how much of
its claim is actually checked.

---

## What counts

Six real, local signals `verus_ir::VerusFnFacts` already tracks, each its
own rule id. A function can carry more than one (e.g. `assume` and
`admit` in the same body) — one finding per active signal per function.

| Kind | Rule id | Real Verus syntax | Why it matters |
| --- | --- | --- | --- |
| Assume | `PROOF-PATTERN-ASSUME` | `assume(cond)` in a body | Trusts `cond` from that point on instead of proving it |
| Admit | `PROOF-PATTERN-ADMIT` | `admit()` in a body | Discharges the entire remaining proof obligation unconditionally — the strongest local escape hatch |
| External body | `PROOF-PATTERN-EXTERNAL-BODY` | `#[verifier::external_body]` | Verus never checks the body against the signature; `ensures` is trusted based on unverified exec code alone |
| Uninterp | `PROOF-PATTERN-UNINTERP` | `uninterp spec fn` (no body) | Nothing backs the fn's meaning except what a caller chooses to trust |
| Axiom | `PROOF-PATTERN-AXIOM` | `axiom fn` | Assumed, not proven — every proof built on it rests on this being true |
| Broadcast | `PROOF-PATTERN-BROADCAST` | `broadcast proof fn` | Applies automatically to every proof in scope via `use`; its contribution to the total proof burden is invisible at call sites |

The first five match `VerusFnFacts::is_trusted_not_proven()` exactly.
Broadcast is tracked separately — it isn't a trust problem, it's a
visibility problem (a caller's proof can depend on a broadcast lemma
without naming it anywhere).

Each finding also carries the function's `tracked_params` and
`recommends` clauses (rendered to text) as informational context —
real signal for what the function's own proof obligation depends on
carrying, and what well-formedness conditions it declares without
requiring the caller to discharge them.

## What doesn't

| Shape | Flagged |
| --- | --- |
| Ordinary `spec`/`proof`/`exec` fn with none of the six signals | no |
| `open`/`closed`/`open(...)` spec fn (visibility only, not trust) | no |
| `requires`/`ensures`/`decreases` clauses on their own | no — informational, not a trust signal |
| A crate with no `verus! { .. }` blocks | no — `verus_ir` finds zero functions |

---

## Design

Consumes `crate::verus_ir::scan_crate_verus_ir` directly (the same real
`verus_syn` parse `panics` merges in) — no separate compiler invocation,
no best-effort recovery. Feature `proof_patterns`, requires `verus_ir`,
folded into `quality`.

Hooks: source loader, scope + inventory + attribute enrichers, probe,
assessor, CSV / checklist / summary reporters — same shape as
`verus_warnings`. `cordial exceptions show proof_patterns`.

| Task | Detail |
| --- | --- |
| `verus_ir` foundational IR (mode, publish, requires/ensures/decreases, assume/admit/external_body, tracked_params, recommends, broadcast) | done |
| Scan + etiquette bundle | done |
| Tests in `tests/proof_patterns_etiquette.rs` | done |

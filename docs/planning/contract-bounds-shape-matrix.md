# Contract-bounds scanner: a shape-matrix test suite

Planning note for hardening `ANTIPATTERN-UNNAMED-CONTRACT-BOUND-001`'s
clause-recognition logic (`src/etiquettes/antipatterns/contract_bounds/`)
against a systematic, growing table of syntactic shapes, instead of the
current one-hand-written-test-per-bug-found style.

## Problem

The scanner's job is narrow but sharp-edged: given a `requires`/`ensures`
clause's raw tokens, decide whether it's a real call to a registered
`amenable_core::Ensures`/`Requires` predicate (silent) or a raw, unnamed
equation (flagged). Three independent front-ends feed it — `kani.rs`
(real `syn::Expr` parsing of `assert!`/`assert_eq!`/`kani::assume`),
`verus.rs` (flat `proc_macro2::TokenStream` walking inside `verus! {}`),
`creusot.rs` (attribute-token walking) — and the shared matcher
(`index.rs`'s `ContractIndex::matches_named_call`, `bare_named_call_name`,
`split_top_level_commas`) works at the token level, not full-expression
grammar, specifically to avoid needing to understand Pearlite/Verus DSL
syntax.

Token-level matching is cheap but structurally fragile: `<`/`>` aren't
real delimiters in Rust's tokenizer (unlike `()`/`{}`/`[]`), so anything
that treats a flat token run as safely comma-splittable can be fooled by
a comma that's lexically top-level but semantically nested inside a
turbofish. Two real bugs of this general shape were found and fixed
during the 2026-08-29 `amenable` contract-bound-naming sweep:

1. **Whitespace-normalization asymmetry**: `RustStdStandard::<std::str::Bytes<'static>>::ensures(...)`
   (nested generic + lifetime) rendered with inconsistent Joint/Alone
   token spacing depending on whether the tokens came from re-lexed
   evidence text or a live `syn` AST — fixed via a symmetric
   `strip_whitespace()` helper in `matches_named_call`.
2. **Dotted-method keyword collision**: `H::default.ensures((), result)`
   (Verus's own builtin function-item contract-inspection syntax) has
   the literal identifier `ensures` inside it — `walk_verus_tokens`
   mistook it for a second clause-list-starting keyword, splitting one
   real clause into two bogus fragments. Fixed by requiring a genuine
   `requires`/`ensures` keyword never be immediately preceded by `.`.

A third, structurally similar bug was found the same way, initially left
**unfixed** and later closed (2026-08-30, see "Why the const-generic case
turned out not to be harder" below):
`RustStdStandard::<std::array::IntoIter<i32, 3>>::ensures(...)` — a
multi-parameter/const-generic turbofish — has a comma *inside* `<i32, 3>`
that isn't inside any `Group` token. `kani.rs`'s `check_macro_call` used
to split `assert!`'s arguments on top-level commas and, seeing exactly 2
segments, assumed it had hit the ordinary `assert!(expr, "message")`
shape: it kept the truncated first segment
(`RustStdStandard::<std::array::IntoIter<i32`) as "the expression" and
silently discarded the second as if it were a message string. The
truncated fragment didn't parse as any valid call shape, so it was
flagged even though the real code was already correctly named.

Both fixed bugs, and the third (also now fixed), were each found by hand
while doing unrelated naming work, then patched with exactly one
hand-written regression test apiece (`tests/contract_bounds.rs`,
originally 25 tests, 969 lines). Nothing enumerated the space of shapes
the scanner is *supposed* to handle, so there was no way to tell "we've
covered the realistic cases" from "we've covered the cases someone
happened to hit."

## Design: the shape matrix

Replace ad hoc test growth with a table of `(verifier, shape, expected
outcome)` rows, driving one shared assertion function instead of one
`#[test]` per case.

### Axes

**Verifier** (`Kani` | `Creusot` | `Verus`) — because each front-end reads
clause text differently; not every shape applies to every verifier
(turbofish variations are Kani-specific real-Rust syntax; `old(self)`/
`final(self)`/`@`/`is`/`->`/`#[trigger]` are Verus-specific DSL forms).

**Shape** — the syntactic pattern under test:

| Shape | Example | Verifiers | Status |
| --- | --- | --- | --- |
| Bare call | `name(args)` | all | handled |
| Negated bare call | `!name(args)` | all | handled |
| Fully-qualified (qself) | `<Type as Trait>::method(args)` | kani | handled |
| Typed-path suffix (abbreviated via `use`) | `Type::method(args)` | kani | handled |
| Turbofish, no internal comma | `Type::<Generic<'a>>::method(args)` | kani | handled |
| Turbofish, comma-bearing generic | `Type::<Generic<A, B>>::method(args)` | kani | handled (fixed 2026-08-30) |
| Turbofish, const-generic | `Type::<Generic<A, 3>>::method(args)` | kani | handled (fixed 2026-08-30, array.rs) |
| Dotted call reusing a keyword-shaped name | `X::default.ensures(args)` | verus | handled (fixed 2026-08-29) |
| `assert_eq!` synthesis | `assert_eq!(A, B)` → `A == B` | kani | handled |
| Trivial: bare `result`/`!result` | `result`, `!result` | all | handled |
| Trivial: tuple projection | `result.N`, `!result.N` | all | handled |
| Trivial: `result.N is None` | — | verus | handled |
| Verus state forms | `old(self).f`, `final(self).f` | verus | handled |
| Verus view/variant forms | `@`, `is`, `->` | verus | handled |
| `#[trigger]`-annotated bare call | `#[trigger] name(args)` | verus | handled |
| Nested generic + lifetime spacing | `RustStdStandard<Bytes<'static>>` | kani | handled (fixed 2026-08-29) |

Every row needs a **positive** case (registered name present → silent)
*and* a **negative** case (same shape, name absent from the registry →
still flagged) — a table that only ever asserts "matches" can't catch a
matcher that's gone too permissive.

### Data shape

```rust
struct ShapeCase {
    id: &'static str,       // e.g. "kani_turbofish_const_generic"
    verifier: Verifier,     // Kani | Creusot | Verus
    kind: &'static str,     // "ensures" | "requires"
    source: &'static str,   // snippet, same style as today's inline tests
    registry: fn() -> Vec<ContractRecordDump>,
    expect_flagged: bool,   // false = should be silent; true = known gap
}
```

One test iterates the table, dispatches to the matching
`scan_{kani,creusot,verus}_contract_bounds_source` (all three already
share the signature `(source, file, src_root, registry) ->
CordialResult<Vec<AntipatternSiteRecord>>`, so dispatch is a single
`match case.verifier`), and asserts `!findings.is_empty() ==
case.expect_flagged` — with `case.id` in the panic message so a broken
row names its own pattern instead of producing a generic assertion
failure.

### The growth loop

This is the point of the exercise: turn "found a bug, patch it, write
one test" into a checklist that can't silently shrink.

1. A real miss surfaces (dogfooding, a user report, or exploratory
   review). Add its row first, with `expect_flagged` set to whatever the
   scanner *currently* does (i.e. red — documents the gap explicitly
   rather than leaving it only in a commit message or memory file).
2. **Fixing now**: implement, flip `expect_flagged` to the correct value,
   confirm green.
3. **Deferring**: leave the row as an accepted-gap marker if a real fix
   is out of scope for now. The table itself becomes the "known
   limitations" list — a future contributor (or session) can grep for
   `expect_flagged: true` and see exactly which shapes are still open,
   with a runnable reproduction attached to each one. (No row is
   currently in this state — see "Why the const-generic case turned out
   not to be harder" below for the case that looked like it would need
   this and didn't.)

### Migration

The existing 25 hand-written tests in `tests/contract_bounds.rs` migrate
into rows first (preserving current coverage, not building a second
parallel suite), then the three documented bugs above get their own rows
as the bootstrap set for "found in the wild."
`tests/contract_bounds.rs`'s existing helpers (`fixtures_root()`,
`logic_fn_record()`, `kani_type_record()`) carry over into the row
`registry` closures largely unchanged.

## Why the const-generic case turned out not to be harder than the other two

The original assessment (below, kept for the record) expected this case
to need real scoped work: a hand-rolled `<`/`>` depth counter can't safely
disambiguate a turbofish's generic-argument list from the comparison/
shift operators (`assert!(a < b, "msg")` must still split into two
arguments) without reimplementing a real slice of Rust's grammar.

That assessment was correct about the *naive* fix being unsafe, but
missed a simpler option: `kani.rs`'s `check_macro_call` was hand-splitting
`assert!`/`assert_eq!`'s raw macro-argument tokens on top-level commas
only because `syn::parse_file` doesn't parse a macro invocation's body at
all (a macro's grammar is macro-specific, so `syn` only ever hands back
the opaque token stream). But `assert!`/`assert_eq!`'s arguments are
always ordinary Rust expressions — there was no need to hand-roll
anything. Parsing `node.tokens` directly via
`syn::punctuated::Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated`
gets the exact same `<`/`>` disambiguation rustc itself performs, for
free, with zero new bespoke logic — because it *is* rustc's real
expression grammar, just invoked through `syn` instead of reinvented.
Fixed in `check_macro_call` (2026-08-30); both `kani_turbofish_const_generic`
and `kani_turbofish_comma_bearing_generic` flipped from `expect_flagged:
true` to `false` and pass. `bare_named_call_name`'s Creusot/Verus token
walk (`index.rs`) and `walk_verus_tokens`'s Verus clause-list splitting
(`verus.rs`) still use the naive token-level split and are NOT covered by
this fix — they can't parse as `syn::Expr` at all (Pearlite/Verus DSL
syntax isn't valid plain Rust), so a comma-bearing generic inside a
`requires`/`ensures` clause list on the Verus/Creusot side remains
unexamined; no known real instance yet.

<details>
<summary>Original assessment (superseded, kept for the record)</summary>

The two already-fixed bugs both had an unambiguous fix: "a `.` immediately
before `ensures`/`requires` is never the keyword" and "normalize
whitespace symmetrically on both sides" are both true in every context,
no exceptions. The const-generic case doesn't have that property: `<`/`>`
aren't unambiguous brackets in Rust's grammar the way `()`/`{}`/`[]` are
— they're also the comparison and shift operators, and appear in bounds,
closures, and ordinary expressions. A naive depth-tracking fix
(increment on `<`, decrement on `>`) would misparse `assert!(a < b, "msg")`
as one nested expression instead of two real arguments, trading a false
positive for a false negative. A correct fix needs enough of Rust's
actual disambiguation heuristic (roughly: `<` opens a generic argument
list only in specific syntactic positions) to be worth scoping as its
own piece of work, not a drive-by patch — the shape matrix's job is to
make that scoping decision **informed**, by first cataloguing exactly
which real shapes in this class exist and how many rows would flip green,
before deciding whether the investment is worth it.

</details>

## Test execution access

Resolved 2026-08-30: for a genuine, requested cordial-repo work session
(as opposed to incidentally wandering into this repo while doing
unrelated `amenable`-side work), full Bash access — `cd`, `cargo
build`/`test`, etc. — is in scope, not just `just install`. `cargo test
--features full --test contract_bounds` was run directly and repeatedly
while building the harness below.

## Status

**Implemented 2026-08-30, known gaps closed same day.** `tests/contract_bounds.rs`
now has a `Verifier` enum, `ShapeCase` struct, and a `SHAPE_CASES` table
(14 rows) driven by one `shape_matrix_matches_expected_flags` test,
dispatching to `scan_{kani,creusot,verus}_contract_bounds_source` and
asserting `!findings.is_empty() == expect_flagged`.

Of the original 25 hand-written tests, 11 were pure boolean flagged/silent
checks and were migrated into table rows (their standalone `#[test]` fns
removed); the remaining 14 assert specific finding counts/content or use a
different entry point (`scan_crate_contract_bounds`, `SessionBuilder`) and
were kept as dedicated tests rather than forced into the coarser boolean
model.

The table's first two "known gap" rows (`kani_turbofish_const_generic`,
the real `array.rs` case, and its synthetic sibling
`kani_turbofish_comma_bearing_generic`) proved short-lived: building the
matrix's `expect_flagged: true` rows immediately surfaced them as a
precisely scoped, reproducible target, which led straight to the real
fix in `check_macro_call` (see "Why the const-generic case turned out not
to be harder" above) rather than staying deferred. Both rows now read
`expect_flagged: false` and pass — exactly the growth loop's "fixing now"
path, just faster than expected.

Re-running the scanner against real `amenable` right after that fix
landed (`just install` + `cordial quality --project ~/repos/amenable
--crate-name amenable_kani`) is what the growth loop calls dogfooding,
and it worked exactly as designed: `array.rs`'s sites were gone, but a
*different* real site (`alloc_collections.rs`'s
`verify_linked_list_extract_if_partitions_by_the_predicate`) was still
flagged — a second, distinct bug in the same neighborhood, a trailing
comma inside the turbofish itself surviving into the type-prefix suffix
match (`RustStdStandard::<..ExtractIf<..>,\n>::ensures(..)`, comma before
the closing `>`, valid and semantically elidable Rust). Added as
`kani_turbofish_trailing_comma`, fixed the same day via
`canonicalize_type_text` collapsing `,>` to `>`, row now
`expect_flagged: false`. `SHAPE_CASES` has 14 rows and zero
`expect_flagged: true` ones.

`cargo test --features full --test contract_bounds`: 15 passed, 0 failed.
`cargo test --features full` (whole crate) and `cargo clippy --features
full`: clean. `cargo fmt --check`: clean. Confirmed against real
production code, not just the fixtures: workspace-wide `cordial quality
--project ~/repos/amenable` now reports `amenable_kani`: 0
`ANTIPATTERN-UNNAMED-CONTRACT-BOUND-001` findings, `amenable_creusot`: 0,
`amenable_verus`: 10 (unchanged — all 10 are separately-documented
genuine exceptions: trigger clauses, a Verus builtin, and
`cfg(windows)`-gated registrations, none of them scanner bugs). Before
this session `amenable_kani` carried the `array.rs` sites as a documented
scanner-limitation exception; it no longer needs one.

Open for a future session: grow `SHAPE_CASES` with the other taxonomy
rows that don't yet have a row (verus state forms, `@`/`is`/`->` view
forms, `#[trigger]`-annotated calls — currently only covered implicitly
by the kept dedicated tests, not as standalone table rows); and whether
the Creusot/Verus side's own naive token-level clause-list splitting
(`bare_named_call_name` in `index.rs`, `walk_verus_tokens` in `verus.rs`)
has the same comma-in-turbofish exposure the two Kani fixes didn't touch
— no known real instance yet (Verus's 10 remaining findings are all
genuine, not this), but nothing rules it out for a future real site.

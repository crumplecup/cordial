# `cfg_hygiene` etiquette

Planning note for the new **cfg-hygiene** quality etiquette: Step 2 of
`amenable`'s `docs/CFG_HYGIENE_PLAN.md`, the general, cordial-side check for
the exact bug class Step 0 of that plan fixed by hand — a workspace-wide
`--check-cfg` union that makes a misplaced verifier cfg name invisible to
`rustc` itself.

---

## Problem

`amenable`'s original `check-cfg` setup declared every verifier's cfg name
(`kani`, `creusot`, `verus_keep_ghost`) "expected" workspace-wide, in every
crate's build. That silences `unexpected_cfgs` everywhere, which sounds
convenient until a name lands in the *wrong* crate — a `#[cfg(creusot)]`
copy-pasted into `amenable_kani`, say. `rustc` itself can never catch that:
the name is declared "expected" there too, by construction. Fixing it (Step 0:
per-crate `build.rs` declarations instead of a union) closed the immediate
hole but created no lasting check against it recurring, and doesn't cover the
general case either — any cfg name, verifier or not, that nothing ever
declared.

Two rules, sibling to `cfg_scatter` (different concern: that etiquette is
about organizational DRY-ness, this is about lint-hygiene correctness):

- **UNEXPECTED-CFG-001**: a `cfg(X)`/`cfg_attr(X, ...)` naming an `X` not
  declared anywhere reachable by that crate.
- **CFG-VERIFIER-MISMATCH-001**: a crate registered in `cordial.toml`'s
  `[cfg_hygiene] crate_verifier` table using a *different* verifier's cfg
  name than its own configured identity — the real gap above, generalized
  and made permanent.

## What "declared" means

Getting this right without false-positiving on ordinary `#[cfg(test)]`/
`#[cfg(feature = "x")]` code took a real, empirically-grounded pass, not a
guess:

- **rustc's own built-in vocabulary** (32 names — `unix`, `windows`,
  `target_os`, `panic`, `doc`, `debug_assertions`, …) — always expected,
  no declaration needed. Verified against a real `nightly` `rustc
  --print=check-cfg` run, not assumed from memory or docs
  (`declared.rs`'s own doc comment carries the exact reproduction command).
- **Cargo's own three injected extras** — `test`, `feature` (the *name*;
  an undeclared *value* is a values-check this etiquette doesn't attempt),
  and `docsrs` — confirmed empirically too
  (`RUSTFLAGS="-D unexpected_cfgs" cargo check` on a throwaway crate). None
  of these three are in rustc's own built-in list; Cargo adds them on top,
  which is exactly why a bare `rustc` invocation (no `cargo`) never
  auto-declares `test` even though it "just works" under ordinary `cargo
  check`.
- **This crate's own `Cargo.toml [lints.rust.unexpected_cfgs] check-cfg`**
  and, if `[lints] workspace = true`, the **workspace's**
  `[workspace.lints.rust...]` equivalent.
- **This crate's own `build.rs`** — a plain text scan for
  `cargo::rustc-check-cfg=cfg(NAME...)` (current syntax) or the legacy
  single-colon form, not a real build invocation.
- **`cordial.toml`'s `[cfg_hygiene] extra_known_names`** — an escape hatch
  for a cfg a `build.rs` computes at runtime (env-var-driven, say), which a
  static scan can never see either way.

All of this is static source scanning, same as every other etiquette here —
it never runs `rustc`/`cargo` itself, so a `build.rs` that computes its
`--check-cfg` list dynamically is an accepted blind spot, not a bug.

## `CFG-VERIFIER-MISMATCH-001`'s scope is deliberately narrow

The obvious-looking rule — "flag any crate using a cfg name it doesn't
own" — is wrong: a verifier's `--cfg` applies to its *whole compiled
dependency graph*, not just the backend crate itself (real precedent:
`cargo kani`'s `--cfg kani` reaches `amenable_core`/`amenable_gaap`/
`amenable`, all legitimate `#[cfg(kani)]` users, none of them
`amenable_kani`). Flagging every non-owning crate would make this etiquette
useless on exactly the workspace it was built for.

Instead: `crate_verifier` in `cordial.toml` is opt-in *per crate* — only
crates actually listed get checked at all (same pattern as
`[tracing].apply_gate_crates`), and the check is narrow: does *this*
crate's own source use a *different* verifier's name than its own
configured identity? Upstream crates that legitimately reference several
verifiers are never registered, so they're never checked; only the backend
crates themselves (`amenable_kani = "kani"`, `amenable_creusot = "creusot"`,
`amenable_verus = "verus"`) are, and only for the specific bug that
motivated this etiquette.

## Design

Follows the standard etiquette skeleton, closer to `antipatterns`'
multi-rule-one-category shape than `cfg_scatter`'s single-rule one (see
`allows`/`antipatterns` for precedent):

- `scan.rs` — one `syn::Visit` overriding only `visit_attribute`, since
  every generated item-kind visitor in `syn::visit` already calls it for
  that item's own `.attrs` — unlike `cfg_scatter`, this scanner doesn't
  care what kind of item the `cfg` sits on, so no per-kind classification is
  needed. Context-tracking overrides (`visit_item_fn`, `visit_item_impl`, …)
  exist only to label *where* an occurrence is, for the checklist. Deliberately
  does **not** skip `#[cfg(...)]` on a `mod` declaration the way
  `cfg_scatter` does — an undeclared name is just as real a bug there.
  Recursively walks `all()`/`any()`/`not()` combinators to extract every
  name a predicate mentions; for `cfg_attr(predicate, attrs...)` only the
  leading `predicate` is a cfg expression.
- `declared.rs` — the built-in/Cargo/`Cargo.toml`/`build.rs`/config
  resolution above, plus `expected_verifier_for`/`all_verifier_names` for
  the second rule.
- `scan_crate.rs` — combines the two into `CfgHygieneSiteRecord`s.
- `types.rs` — `CfgHygieneRuleId` (two variants, one `Rule` category
  `cfg_hygiene`, `AntipatternRule`'s exact shape), `CfgHygieneFinding`.
- `enricher.rs`/`probe.rs`/`assessor.rs` — the standard triple, generic
  over both rule ids (`AntipatternAssessor`'s shape, not `cfg_scatter`'s
  single-rule one).
- `reporter.rs` — `cfg-hygiene.csv`, `cfg-hygiene.checklist.md`,
  `cfg-hygiene-summary.md`. The checklist and summary group findings by each
  finding's own `crate` field (`antipatterns`' summary pattern) rather than
  the crate currently being rendered — `cfg_scatter`'s own checklist/summary
  reporters only ever show the render-time crate's single row, which is a
  real gap in a multi-crate workspace, caught here by a dedicated
  two-member-workspace regression test
  (`cfg_hygiene_etiquette_summary_and_checklist_group_by_each_finding_own_crate`)
  before it could repeat.

New feature: `cfg_hygiene = ["dep:toml"]`, folded into `quality`.

## Dogfood results

Zero findings on both real targets, both rules:

- **`cordial` itself** (`cordial quality`, `full` features): 0/0. A large,
  feature-flag-heavy codebase with no `crate_verifier` config (the rule is
  inert without it) — the meaningful signal here is the *absence* of false
  positives on ordinary `#[cfg(feature = "x")]`/`#[cfg(test)]` code, not a
  finding count.
- **`amenable`** (the motivating workspace, real `crate_verifier =
  { amenable_kani = "kani", amenable_creusot = "creusot", amenable_verus =
  "verus" }`): 0/0. Confirms two things at once — `amenable`'s Step 0
  per-crate `build.rs` declarations are complete (no undeclared names
  anywhere), and Step 1's `unexpected_cfgs`-suppression fix (wrapping four
  macro-injection sites in `#[allow(unexpected_cfgs)] const _: () = { ... };`)
  never let a verifier's cfg name leak into the wrong backend crate.

## Status

Implemented and tested (`tests/cfg_hygiene_etiquette.rs`, 6 tests: raw-scan
combinator extraction, undeclared-vs-builtin, `build.rs`-declared,
verifier-mismatch-vs-own-identity, multi-crate summary/checklist grouping,
default thresholds). `cargo fmt --check`/`clippy --features full -D
warnings`/`just test` (full suite, `full` features) all clean; `cargo check
--no-default-features` compiles (pre-existing, unrelated dead-code warnings
only, expected with most etiquettes off). Not yet done: an `--apply` for
either rule (neither is auto-fixable the way tracing instrumentation is —
both need a human decision about where the missing declaration or the
misplaced cfg actually belongs).

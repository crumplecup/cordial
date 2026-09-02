# Tracing etiquette

Upgrade the tracing etiquette from a missing-`#[instrument]` census to a
**classified recommendation**: every function gets a use-class, a
complexity, and a target recipe. Apply writes that recipe. Volume is a
subscriber `level` problem, not a reason to skip spans. Visibility is not
a filter.

Companion to [derives](elicit-doc-parity.md) (manual getter/setter/`new` —
different question) and [error-handling-as-plugin.md](error-handling-as-plugin.md)
(error-site kinds on the same IR). Tracing **consumes** those facts; it does
not re-derive them.

Thresholds live in `cordial.toml` under `[tracing]`. See
[cordial-config.md](cordial-config.md).

---

## Problem

Dogfood (`quality` on this repo) reports **467** open gaps and **0**
exceptions. Detection of a missing attribute is correct (`run_session` and
`build_shadow_report` are excluded once `#[instrument]` is present). The
output is still a poor *catalog*:

- One rule (`TRACING-MISSING-INSTRUMENT`) for constructors, path getters,
  scanners, and session entry points.
- Apply stamps `#[instrument]` or `#[instrument(skip(...))]` at the default
  **info** level. No `level`, `err`, `ret`, or `fields`.
- Checklist dumps every crate under `ir.crate_name()` (visibility already
  fixed this for its reporters). Paths are absolute; apply’s tests use
  crate-relative paths.
- `record_fn` already receives `sig` and `attrs` and discards them
  (`let _ = (sig, attrs)`).
- This crate never uses `#[instrument(err)]`. Fallible functions are spanned
  the same as `StoreLayout::cache_dir`.

The etiquette’s job is engineering excellence: **instrument this function
properly for its class**, then look the gaps up like a table — not a novel.

---

## Design

Closed **`FunctionRole`** enum. Classify once. **Match on the variant** to
pick a strategy that returns an **`InstrumentRecipe`**. Assess compares the
recipe to whatever `#[instrument]` (and body events) are already there.

```text
scan  →  classify(role, complexity)  →  IR attrs
probe →  missing / mismatch markers
assess →  findings (rule + recipe)
report →  CSV / checklist / summary grouped by crate then role
apply  →  match role again, write the recipe
```

Do not introduce a strategy trait object in v1. A `match` on `FunctionRole`
is the dispatch. If a variant’s recipe function outgrows the file, peel it
to `tracing/strategy/{role}.rs` the same way other etiquettes grow named
layers — still called from one match.

### `FunctionRole`

Syntactic `FunctionKind` (`free` / `inherent` / `trait_impl`) stays as
structure. Role is **use**:

| Variant | Heuristic (first match wins) |
| --- | --- |
| `Constructor` | `new`, `try_new`, `default`, `from_*` that returns `Self` |
| `Getter` | `&self`, not mut, not `Result`; name is `as_*` / `to_*` / `id` / `*_dir` / `*_path` / `*_name`, or a trivial field accessor. Free functions are never getters. |
| `Setter` | `&mut self` or fluent `Self`; `set_*` / `with_*` |
| `Predicate` | `is_*`, `has_*`, `can_*`, `contains_*`; returns `bool` |
| `Scan` | `scan_*`, `walk_*`, `visit_*`, `collect_*` over syntax/IR |
| `Io` | `load_*`, `read_*`, `write_*`, `fetch_*`, `ensure_dirs` |
| `Render` | `render_*`, or name ends `_summary` / `_checklist` / `_csv` |
| `TraitSurface` | `function_kind = trait_impl` (e.g. `Rule::id`) |
| `Entry` | crate-visible pipeline: `run`, `run_*`, `main` |
| `Other` | remainder |

Role classification must not require rustdoc. Signature + name + a cheap
body peek (return type, receiver, statement count) is enough. Prefer
sharing getter/setter/`new` predicates with the derives scan rather than
forking a second grammar.

### `FunctionComplexity` (orthogonal)

| Variant | When |
| --- | --- |
| `Trivial` | handful of statements, field access / `join` / `to_string` |
| `Linear` | no `Result` return, no `match`/`if` beyond a single path |
| `Branchy` | control-flow beyond a single happy path |
| `Fallible` | returns `Result` or a `*Result` alias (`CordialResult`, …). `?` on `Option` is not Fallible. |
| `Hotspot` | body lines ≥ modularity `function_inventory_min_lines` (150) |

A function can be `Getter` + `Trivial`, or `Scan` + `Fallible` + `Hotspot`.
Role selects the strategy; complexity and attached context are arguments
the strategy reads.

### `FnContext` (what each strategy sees)

- role, complexity, visibility
- param names (already parsed for `skip`)
- return type (`Result`, `Self`, `bool`, …)
- error-site kinds on this fn (join error-sites / error-ir; do not rescan)
- existing `#[instrument]` args (`level`, `skip`, `err`, `ret`, `fields`)
- existing `tracing::{error,warn,info,debug,trace}!` in the body
- body line count

### `InstrumentRecipe`

The target attribute (and events the attribute cannot express):

```text
level:   trace | debug | info | warn
skip:    param names (self, ir, findings, … — existing skip list)
fields:  identity args worth recording (crate_name, path, …)
err:     None | Some(level)     // #[instrument(err)] / err(level = "warn")
ret:     bool                   // constructors, cheap predicates
events:  extra warn!/error! if err is not enough (explicit policy)
```

**Dispatch** (one match, one function per variant):

```rust
fn recipe(role: FunctionRole, ctx: &FnContext) -> InstrumentRecipe {
    match role {
        FunctionRole::Constructor => constructor_recipe(ctx),
        FunctionRole::Getter => getter_recipe(ctx),
        FunctionRole::Setter => setter_recipe(ctx),
        FunctionRole::Predicate => predicate_recipe(ctx),
        FunctionRole::Scan => scan_recipe(ctx),
        FunctionRole::Io => io_recipe(ctx),
        FunctionRole::Render => render_recipe(ctx),
        FunctionRole::TraitSurface => trait_surface_recipe(ctx),
        FunctionRole::Entry => entry_recipe(ctx),
        FunctionRole::Other => other_recipe(ctx),
    }
}
```

Default recipes (strategies may raise level when `complexity` is `Hotspot`
or `Fallible`):

| Role | level | err | ret | notes |
| --- | --- | --- | --- | --- |
| `Constructor` | debug | if `Result` | yes | `fields` for the distinguishing arg |
| `Getter` | trace | if `Result` | no | name prefixes require `&self`; `Result` `to_*` is not a getter |
| `Setter` | trace | if `Result` | no | |
| `Predicate` | trace | no | optional | |
| `Scan` | debug | if `Result` | no | skip syntax/IR blobs |
| `Io` | info | warn on `Err` | no | skip handles, paths optional as fields |
| `Render` | debug | if `Result` | no | skip `findings`, `body` |
| `TraitSurface` | trace | no | no | `id` / `category` / `fmt` |
| `Entry` | info | warn on `Err` | no | `fields` for crate/project |
| `Other` | debug | if `Result` | no | |

`Fallible` (Result return) asks for `err` at **warn** unless the strategy sets
otherwise. `#[instrument(err)]` only emits on `Err`. `Option` + `?` is
absence, not a silent error.

Do not omit getters from the inventory. Filter at the subscriber
(`RUST_LOG=info` vs `trace`).

---

## Rules

Findings are **recipe deltas**, not a second census of every function, plus
**attenuation** rules that fire when `#[instrument]` is already present
where a verifier backend cannot (or must not) see it. Missing-instrument
alone pushes only toward more spans; without the counter-lints a crate
can accumulate tracing on proof-only `Requires`/`Ensures` impls, or leave
bare `#[instrument]` on Kani-reachable production code.

| Rule | When |
| --- | --- |
| `TRACING-MISSING-INSTRUMENT` | Function with no `#[instrument]`. Carry the recipe so apply knows what to write. Not raised for proof-only functions or skip-policy files. |
| `TRACING-LEVEL-MISMATCH` | Span present, recorded `level` (default **info**) is higher-volume than the recipe (e.g. getter at info). |
| `TRACING-SKIP-MISSING` | Recipe `skip` names are live params and absent from `skip(...)`. |
| `TRACING-ERR-MISSING` | Recipe wants `err` (fallible / `Result`) and the attribute has neither `err` nor `err(level = ...)`. |
| `TRACING-ERROR-PATH-SILENT` | Recipe wants `err`, the attr has no `err`, and the body has no `warn!`/`error!`. Not fired for `Option` lookups or non-Result error-site joins. |
| `TRACING-FIELDS-MISSING` | Recipe `fields` empty on an `Entry`/`Constructor` that has a clear identity param. |
| `TRACING-PROOF-INSTRUMENT` | Function is proof-only (nested in `#[cfg(<gate>)]` / `#[<gate>::…]`, or every known in-workspace caller is) **and** already has `#[instrument]` — including `#[cfg_attr(not(kani), …)]`. Gating is not a fix: the function never runs outside the prover, so the span never fires. Apply **removes** the attribute. |
| `TRACING-UNGATED-INSTRUMENT` | File compiles under a gate-policy crate (Kani, etc.), the function is ordinary (not proof-only), and the attribute is **bare** `#[instrument]`. The prover will expand it. Apply rewrites to `#[cfg_attr(not(<cfg>), tracing::instrument(..))]`. |
| `TRACING-SKIP-INSTRUMENT` | File is skip-policy (Verus bare compiler / Creusot translator, including `#[path]` splices) **and** already has `#[instrument]`. Apply **removes** it. Uninstrumented skip-policy functions are silent (no missing-instrument push). |

Private, `pub(super)`, `pub(crate)`, and `pub` are all on the checklist.
`TraitSurface` is in so inherited vis on trait impls still gets a `trace`
span without flooding INFO.

Documented exceptions keep working (JSON patches). Exceptions are for
policy, not for “this getter would be noisy.”

---

## Join other etiquettes

| Source | Use |
| --- | --- |
| Derives scan predicates | Constructor / getter / setter shape — same grammar, different finding |
| Error-sites / error-ir | consumed for body `warn!`/`error!` events; does not mark non-Result fns silent |
| Modularity body lines | `Hotspot` complexity |
| Attribute enricher | Current `#[instrument]` args (parse `level`, `skip`, `err`, `ret`, `fields`) |

Quality plugin order already loads source + attributes. Prefer reading IR
attrs / markers over a second walk. If error-sites is not in the session,
fallible is signature-only (`Result` return).

---

## Artifacts

Keep the three names. Change the *schema*.

- **`tracing-instrument.csv`** — one row per finding: crate, qualified
  name, role, complexity, rule, recipe (`level`, `skip`, `err`, `ret`),
  file (crate-relative), line, disposition.
- **`tracing-instrument.checklist.md`** — group **crate → role → module**.
  Each open item states the recipe in the same shape apply will write,
  e.g. `` `#[instrument(level = "trace", skip(self))]` ``. Relative paths
  so apply can `crate_root.join(rel_path)`.
- **`tracing-summary.md`** — workspace table: counts by crate **and**
  role; open vs suppressed. Not a single `ir.crate_name()` row.

Reporters must group by the finding `crate` field (same fix as visibility).

Apply (`run_tracing_instrument_apply`) **re-classifies** at the fn (do not
parse the recipe out of markdown as the source of truth). Checklist text
is for humans; dispatch is the same `match` used in assess. Write the full
attribute: `level`, `skip`, `err`/`ret`/`fields` as the strategy says.
Keep `ensure_use_instrument` and the skip-name list; skip names become an
input to the recipe, not the only knob.

---

## Config

```toml
[tracing]
# Default skip param names (union with built-in list).
# extra_skip = ["inventory"]
# Crate -> cfg: --apply writes #[cfg_attr(not(<cfg>), tracing::instrument(..))]
# apply_gate_crates = { amenable_kani = "kani" }
# Never write #[instrument] (bare Verus / Creusot translator).
# apply_skip_crates = ["amenable_verus", "amenable_creusot"]

[tracing.subscriber]
# All default true. Turn a knob off to silence that rule.
# init_in_main = true
# init_in_tests = true
# helper_in_lib = true
# rust_log_fallback = true
# idempotent = true

[tracing.boundary]
# Default true.
# main_reports_errors = true
# Cross-crate dispatch helper(s) trusted as already reporting their own
# errors (same shape as [tracing.subscriber]'s knob of the same name).
# known_helper_paths = ["amenable_core::run_and_report"]

[tracing.stdio]
# All default true. One lint per leftover stdio macro.
# println = true
# eprintln = true
# print = true
# eprint = true
# dbg = true
# skip_cargo_protocol = true
# skip_folders replaces the default list (does not union):
# skip_folders = ["tests/fixtures", "tests/parity"]
```

Role→level maps stay in code for v1 (the enum is the policy). Promote to
TOML only if dogfood needs a project override. Do not add a visibility
filter or an “ignore getters” flag; those are the hatchets this upgrade
removes. Filter at the subscriber (`RUST_LOG=info` vs `trace`).

`err()` is only recommended when the `Err` type is known `Display` from
the same file (or is a well-known `String`/`Error`). Functions nested in
an ancestor `#[cfg(<gate>)]` / `#[<gate>::…]` (and functions whose every
known in-workspace caller is already in that set) are **proof-only**:
never recommended for a span, and `TRACING-PROOF-INSTRUMENT` if one is
already there — including a `not(kani)` gate, which would never fire.

---

## Subscriber init

Instrument coverage is useless if nothing installs a subscriber. Five
rules, default **on**, each a boolean under `[tracing.subscriber]`. Same
`tracing` feature and etiquette; **separate artifacts**
(`tracing-subscriber.checklist.md`) so `--apply` never rewrites these
rows.

Recognize an install (syn, no rustc): a call whose path ends in `init` /
`try_init` / `set_global_default`, or a crate-local helper whose body
contains one of those. `main` / `#[test]` in `tests/` should **call** that
helper by name. `from_default_env()` alone does not count as a `RUST_LOG`
fallback (it panics if the var is unset); use
`EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))`
or `env::var("RUST_LOG")` plus a default.

| Rule | When | Fix |
| --- | --- | --- |
| `TRACING-SUBSCRIBER-MAIN` | Bin has `fn main` that never calls an init helper | Call the lib helper from `main` |
| `TRACING-SUBSCRIBER-TEST` | A `#[test]` in `tests/` never calls that helper | Call the same helper |
| `TRACING-SUBSCRIBER-LIB` | The fn that builds/installs the subscriber lives in `main.rs` / `src/bin/` / `tests/`, not the lib | One documented helper in the library |
| `TRACING-SUBSCRIBER-RUST-LOG` | That helper does not read `RUST_LOG` **and** have a fallback | `try_from_default_env` + `unwrap_or*` (or `env::var("RUST_LOG")` plus a default) |
| `TRACING-SUBSCRIBER-IDEMPOTENT` | Helper uses `init()` without `Once` / `OnceLock` | `try_init()` (ignore already-set) or wrap in `Once` |

Scope:

- Lib-only crates: MAIN is N/A.
- Bin-only (no lib): LIB is N/A.
- Skip/gate verifier crates (`apply_skip_crates` / `apply_gate_crates` on
  **that crate's name**): skip MAIN/TEST — not logging programs.
- Tests: per `#[test]` in `tests/`, not “file called init once.” `src/`
  unit tests stay the inline-tests etiquette’s problem.

Attenuation among the five: inline `fmt().init()` in `main` satisfies MAIN
and fails LIB. Helper in lib that `main` never calls fails MAIN. Helper
copied in main and tests fails LIB.

`CLI-MAIN-001` allows `main` to call that library helper once, then
parse / `act` / miette. Extra items in `main.rs` (a local `fn init_tracing`)
are still a fat main.

---

## Binary error boundary

A library propagates errors up via `?` — that's the existing error-chain
policy, unchanged. A binary's `fn main` is the process boundary: an `Err`
that reaches it unreported is the equivalent of crashing, not reporting
to the user, for a project that has locked its I/O down to tracing events
(`TRACING-STD-*` above disarms `print!`/`dbg!`, so tracing is the one
designated UI channel left). One rule, default **on**, under
`[tracing.boundary]`. Same `tracing` feature and etiquette; **separate
artifact** (`tracing-boundary.checklist.md`) so `--apply` never rewrites
these rows (this is a design signal, not a rewrite the tool can pick a
`level` for on its own).

| Rule | When | Fix |
| --- | --- | --- |
| `TRACING-BOUNDARY-MAIN-SILENT` | Bin has a fallible `fn main` (`-> Result<_, _>`) that neither carries `#[instrument(err(...))]` nor emits `tracing::warn!`/`error!` on its error path, nor delegates to a function in this crate that does either | Add `#[instrument(err(level = "warn"))]` to `main`, or handle the error and emit `tracing::warn!`/`error!` before returning |

Recognize "reports its error" the same way `TRACING-ERR-MISSING` reads an
existing recipe (syn, no rustc): `#[instrument(err)]` / `#[instrument(err(...))]`,
including the `#[cfg_attr(pred, instrument(...))]` gated form, or a direct
`tracing::warn!`/`error!` (bare `warn!`/`error!` too) anywhere in the
function body. `main` delegating to a helper elsewhere in the crate that
itself reports (e.g. `Cli::act`) satisfies the rule without requiring
`err(...)` on `main` a second time — mirrors subscriber's helper-name
delegation, scoped to this crate's own scan. A cross-crate dispatch
helper this crate's scan can't see the body of is trusted only if named
in `known_helper_paths`, same shape as `[tracing.subscriber]`'s knob of
the same name.

Scope:

- `fn main` that can't return `Err` (`-> ()`, `-> ExitCode` via
  `std::process::exit`) has nothing to report — not flagged.
- Skip/gate verifier crates (`apply_skip_crates` / `apply_gate_crates` on
  **that crate's name**): skipped — not logging programs.
- Library-only crates (no `src/main.rs` / `src/bin/`): N/A, nothing to
  check.

---

## Std print

Leftover stdio is a diagnostic that should be a tracing event — including
`main`, `src/cli`, and `tests/`. One lint per macro, same `tracing`
feature and etiquette; **separate artifacts**
(`tracing-print.checklist.md`) so `--apply` never rewrites these rows.
The filter is `[tracing.stdio]` in `cordial.toml`.

| Rule | Macro | Fix |
| --- | --- | --- |
| `TRACING-STD-PRINTLN` | `println!` | `tracing::info!` / `debug!` |
| `TRACING-STD-EPRINTLN` | `eprintln!` | `tracing::warn!` / `error!` |
| `TRACING-STD-PRINT` | `print!` | `tracing::info!` / `debug!` |
| `TRACING-STD-EPRINT` | `eprint!` | `tracing::warn!` / `error!` |
| `TRACING-STD-DBG` | `dbg!` | `tracing::debug!` |

Knobs (all default **on** except folder list):

- `println` / `eprintln` / `print` / `eprint` / `dbg` — arm that lint
- `skip_cargo_protocol` — skip first-string `cargo:` / `cargo::`
- `skip_folders` — crate-relative prefixes. Default
  `["tests/fixtures", "tests/parity"]`. A TOML list **replaces** the
  default, it does not union. Per-site suppressions stay
  `cordial exceptions add`.

Dogfood: cordial `src/` and `tests/` have none. Command payloads (explain
pages, JSON, file dumps) write to stdout with `write!` / `write_all`,
not leftover stdio macros.

---

## Module layout

```text
src/etiquettes/tracing/
  types.rs          FunctionRole, FunctionComplexity, InstrumentRecipe, rules
  classify.rs       classify(sig, name, kind, body peek) -> (role, complexity)
  recordable.rs     which params/returns tracing can record without extra bounds
  recipe.rs         recipe(role, ctx) match + per-variant fns
  scan.rs           keep inventory; stop discarding sig/attrs; record role
  enricher.rs       IR attrs: role, complexity, instrumented, instrument_meta
  probe.rs          missing + mismatch queries
  assessor.rs       compare recipe vs present
  reporter.rs       crate → role grouping; relative paths
  apply/            classify + recipe match; write attribute
  subscriber/       init-helper policy (MAIN/TEST/LIB/RUST-LOG/IDEMPOTENT)
  boundary/         binary error-boundary policy (MAIN-SILENT)
  print/            leftover stdio macros (TRACING-STD-PRINTLN / … / DBG)
```

If `recipe.rs` grows past the modularity file floor, peel
`strategy/{constructor,getter,...}.rs` and keep the match in `recipe.rs`.

---

## Phases

1. **Classify + recipe on findings** — `FunctionRole` / complexity / recipe
   attrs; CSV + checklist + summary by crate and role; relative paths.
   Missing-instrument still the only *lint*; every row carries the recipe.
2. **Delta rules** — level mismatch, skip missing, `err` missing, silent
   error path (error-site join when the layer is present).
3. **Apply writes the recipe** — same match as assess; `level` / `err` /
   `ret` / `fields`; tests for getter→trace and `Result`→`err(level = "warn")`.
4. **Config + dogfood** — `[tracing]` knobs if needed; self-scan; update
   quality-report blurb from “N gaps” to “N gaps by role.”

Parity vs elicit_doc stays “detect missing instrument” for Tier A fixtures.
New rules are cordial-only; freeze extra baselines under `tests/` rather
than weakening fixture recall.

---

## Status

**Phase 4 complete**, plus verifier **attenuation**, **subscriber init**,
and the **binary error boundary**. Classify + recipe on every finding;
delta rules vs present `#[instrument]`; apply writes the recipe.
Counter-lints (`TRACING-PROOF-INSTRUMENT`, `TRACING-UNGATED-INSTRUMENT`,
`TRACING-SKIP-INSTRUMENT`) fire when a span is already present where the
backend cannot use it. Subscriber rules (`TRACING-SUBSCRIBER-*`) live on
the same etiquette with their own checklist. `TRACING-BOUNDARY-MAIN-SILENT`
(a fallible binary `fn main` that never reports its error via tracing
before the process boundary) is a fourth checklist, same delegation
strategy as subscriber's `known_helper_paths`. Leftover stdio
(`[tracing.stdio]`, one rule per macro) is a fifth checklist; `--apply`
does not rewrite subscriber, boundary, or std-print rows. `[tracing]`
knobs (`extra_skip`, `apply_gate_crates`, `apply_skip_crates`,
`[tracing.subscriber]`, `[tracing.boundary]`, `[tracing.stdio]`) load
through `cordial.toml`. The quality report blurb is open gaps **by
role**, plus subscriber, boundary, and std-print counts.

Every function is on the instrument checklist. Visibility is recorded on
the finding; it does not suppress a gap. Role recipes pick `trace` /
`debug` / `info` so subscribers control volume.

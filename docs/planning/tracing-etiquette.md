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

Findings are **recipe deltas**, not a second census of every function.

| Rule | When |
| --- | --- |
| `TRACING-MISSING-INSTRUMENT` | Function with no `#[instrument]`. Carry the recipe so apply knows what to write. |
| `TRACING-LEVEL-MISMATCH` | Span present, recorded `level` (default **info**) is higher-volume than the recipe (e.g. getter at info). |
| `TRACING-SKIP-MISSING` | Recipe `skip` names are live params and absent from `skip(...)`. |
| `TRACING-ERR-MISSING` | Recipe wants `err` (fallible / `Result`) and the attribute has neither `err` nor `err(level = ...)`. |
| `TRACING-ERROR-PATH-SILENT` | Recipe wants `err`, the attr has no `err`, and the body has no `warn!`/`error!`. Not fired for `Option` lookups or non-Result error-site joins. |
| `TRACING-FIELDS-MISSING` | Recipe `fields` empty on an `Entry`/`Constructor` that has a clear identity param. |

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
```

Role→level maps stay in code for v1 (the enum is the policy). Promote to
TOML only if dogfood needs a project override. Do not add a visibility
filter or an “ignore getters” flag; those are the hatchets this upgrade
removes. Filter at the subscriber (`RUST_LOG=info` vs `trace`).

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

**Phase 4 complete.** Classify + recipe on every finding; delta rules vs present
`#[instrument]`; apply writes the recipe. `[tracing]` knobs (`extra_skip`) load
through `cordial.toml`. The quality report blurb is open gaps **by role**.

Every function is on the checklist. Visibility is recorded on the finding;
it does not suppress a gap. Role recipes pick `trace` / `debug` / `info` so
subscribers control volume.

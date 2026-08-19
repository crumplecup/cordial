# `cfg_scatter` etiquette

Planning note for the new **cfg-scatter** quality etiquette: a static lint
for the `#[cfg(feature = "...")]` sprawl antipattern uncovered while
refactoring [error-handling-as-plugin.md](error-handling-as-plugin.md)'s
`error_ir` visitor into mod-gated layers.

---

## Problem

Cordial's own source repeatedly shows the same shape: a single `#[cfg(...)]`
predicate (often a long `any(feature = "a", feature = "b", ...)` list) copy-pasted
onto an import, several free functions, a struct, and an impl block, all in one
file — instead of gating the whole thing once at a `mod` declaration. This is
exactly the pattern fixed by hand in `src/enricher/mod.rs` and
`src/etiquettes/error_ir/` during the mod-gated-layers work. It's tedious to
review by eye across a large, agent-authored codebase, so it belongs as a
static check rather than something caught ad hoc in review.

## What counts as the antipattern (and what doesn't)

- **Not flagged:** `#[cfg(...)]` on struct/enum **fields** or **variants**,
  regardless of count. Once one field holds a feature-gated type, gating that
  field (or several fields on the same struct) is often unavoidable and isn't
  evidence that logic should move into its own module.
- **Flagged:** the same predicate applied to **multiple distinct item kinds**
  in one file — free functions, impl methods, whole structs/enums/traits/impls,
  consts, statics, type aliases, imports, or match arms. Two distinct kinds
  sharing one predicate is enough; so is five-plus occurrences of one kind
  (e.g. ten separate gated free functions in a file).
- `#[cfg]` on a `mod` declaration is never scanned — that's the pattern this
  lint recommends, not the problem.

This mirrors the field-vs-item distinction the user identified: cheap,
type-driven cfg on data members is a false positive; cfg spread across
free-standing logic is the real "should be its own module" signal.

## Design

Follows the standard etiquette skeleton (`scan.rs` → `enricher.rs` → `probe.rs`
→ `assessor.rs` → `reporter.rs`, see `allows`/`modularity` for precedent):

- `scan.rs` — single `syn::Visit` per file. Collects every non-`mod`
  `#[cfg(...)]` occurrence (skipping `#[cfg(test)]`), keyed by the normalized
  predicate text within that file. Groups sharing a predicate are flagged via
  `CfgScatterGroup::is_scatter`.
- `types.rs` — `CfgSiteKind` (the classification above), `CfgScatterThresholds`
  (`min_distinct_kinds: 2`, `min_occurrences: 5` by default), and the usual
  `Rule`/`Marker`/`Finding` triple (`CFG-SCATTER-001`).
- `enricher.rs`/`probe.rs`/`assessor.rs` materialize one IR node + finding per
  `(file, predicate)` group (file-level aggregate anchor, like
  `ModularityKind::File`), carrying the distinct kinds, occurrence count, and
  a handful of sample sites for the checklist.
- `reporter.rs` — `cfg-scatter.csv`, `cfg-scatter.checklist.md`,
  `cfg-scatter-summary.md`.

New feature: `cfg_scatter = []`, folded into `quality`.

## Dogfood results

Running `cordial quality` on cordial's own `src/` with the default thresholds
immediately surfaced ~22 findings (no manual tuning needed), all fixed or
triaged in one pass. Two repeatable fix patterns emerged, both cheaper than
extracting a whole new file:

1. **Inline mod-gated layer**: wrap the scattered items (structs, impls,
   statics, free functions) in a private `mod foo { use super::*; ... }`
   block gated once, and call back in through `foo::item(...)` — no new file
   needed, and the mod declaration itself is never scanned. Used in
   `src/etiquettes/framework_std/{mod.rs,types.rs}`, `src/framework_std/mod.rs`
   (etiquettes), `src/plugins/mod.rs`, `src/reporter/coverage_summary.rs`,
   `src/enricher/error/{mod.rs,inventory.rs,scan.rs}`.
2. **Merge sibling re-exports into one nested `use` tree**: Rust's `use`
   syntax allows disjoint-root groups (`use { a::b, c::d::e };`), so N separate
   `#[cfg(feature = "x")] pub use ...;` lines pulling from N different
   submodules collapse into one `#[cfg(feature = "x")] pub use { ... };`.
   Used in `src/lib.rs`, `src/framework_std/mod.rs` (top-level), and
   `src/testing/mod.rs`.

Files fixed: `src/enricher/mod.rs` (original demonstration),
`src/etiquettes/framework_std/{mod.rs,types.rs}`, `src/framework_std/mod.rs`,
`src/testing/mod.rs`, `src/lib.rs`, `src/plugins/mod.rs`,
`src/etiquettes/error_ir/visitor.rs`, `src/enricher/error/{inventory.rs,scan.rs,mod.rs}`,
`src/plugin/coverage.rs` (partial), `src/reporter/coverage_summary.rs`,
`src/session.rs`, `src/ir/workspace.rs`.

No false positives from field-only gating (e.g. `ErrorIrFileScan`'s
`#[cfg(feature = "error_chain")]`/`#[cfg(feature = "internal_error_chain")]`
fields) showed up in the output.

### Accepted residuals: registry/dispatch fan-out

A handful of findings remain by design, all sharing one shape: a **plugin or
subcommand registry** where each enabled feature contributes exactly one
match arm (or one array slot) to a shared dispatch function, plus the one
`use` needed to name its type. Examples: `src/plugins/mod.rs`
(`coverage_targets_for_plugin`, `coverage_plugins_for_hub` — one arm per
coverage plugin), `src/plugin/coverage.rs` (`Coverage::classify_gap` /
`GapContext::gap_kind` — a trait default method plus a struct method, each
the natural home for that behavior), `src/enricher/error/scan.rs`
(`ErrorIrScanReport::internal_report` — a single conversion method whose
return type only exists under the feature), `src/ir/view.rs`
(`IrMut::workspace_wrapper_coverage` — a single trait default method plus the
`use` needed to name its return type), and `src/cli.rs`
(subcommand variants/arms for `Build`/`Sysroot`).

These don't reduce further without either (a) forcing an artificial module
split that fragments a trait/struct's natural definition site, or (b)
building a heavier dynamic-registration mechanism (e.g. an `inventory`-style
registry) just to avoid one `match` arm per plugin — not worth it for the
gain. `min_distinct_kinds: 2` means any 2-occurrence "one import + one arm/fn"
pair trips the lint regardless of whether it can be reduced; that's a known
sharp edge of the current threshold, not a bug. Left as-is and worth revisiting
if the lint should special-case single-arm/single-fn dispatch sites.

### Scanner fix: trait default methods and impl-associated items were invisible

A second review of the scanner (prompted by "do you see any improvements we
need to make to the scatter scanner?") found a real coverage gap, not just a
threshold-tuning question: `CfgScatterVisitor` only overrode
`visit_item_fn`/`visit_impl_item_fn`/the various `visit_item_*` methods. It
never overrode `visit_trait_item_fn`, `visit_trait_item_const`,
`visit_trait_item_type`, `visit_impl_item_const`, or `visit_impl_item_type`,
so `#[cfg(...)]` on a trait default method (or an associated const/type)
never counted as an occurrence at all. This was caught by hand: the sample
snippet for `src/plugin/coverage.rs`'s `impl_coverage` finding never listed
`Coverage::classify_gap` (a `#[cfg(feature = "impl_coverage")]` default
method), even though it's clearly gated the same way as its neighbors.

Fixed by adding a `CfgSiteKind::TraitFn` kind and the five missing `Visit`
overrides (associated consts/types fold into the existing `Const`/`TypeAlias`
kinds since they're the same shape as free items, just impl/trait-scoped).
Covered by a new regression test,
`scan_cfg_scatter_rust_source_flags_trait_default_methods`. Re-running the
self-scan after the fix: `src/plugin/coverage.rs`'s finding grew from 3 to 4
occurrences (now includes `classify_gap`), and one previously-invisible
finding appeared, `src/ir/view.rs`'s `IrMut::workspace_wrapper_coverage` —
another single-trait-method-plus-import pair, added to the accepted-residuals
list above rather than restructured.

## Status

Implemented and tested (`tests/cfg_scatter_etiquette.rs`, 4 tests). First full
dogfooding pass complete: all mechanically-fixable scatter in `src/` is
resolved; the remaining 8 findings (7 original + `src/ir/view.rs`, newly
visible after the trait-item scanner fix) are accepted registry/dispatch
residuals (see above). `cargo check`/`clippy`/`test` pass across `full`, each
touched feature in isolation, and `--no-default-features`.

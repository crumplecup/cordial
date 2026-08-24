# Quality-report feeder trait

`quality-report.md`'s resolution-order rollup was a second, independently
hand-maintained list of "every quality etiquette" — separate from
`quality_etiquettes()`, the list that actually registers etiquettes onto
a session. `proof_patterns` shipped fully wired (CSV, checklist, summary,
tests all passing) and was still silently absent from the rollup, because
nobody added a matching block to `build_quality_report`'s hardcoded area
list. A second, dormant instance of the same bug was found while fixing
the first: `foreign_error_types`'s own "chain break" rule id was never
referenced anywhere in the rollup either — invisible only because it
happened to always be zero.

## The real invariant

Not "every etiquette gets exactly one area" — that's false. Some
etiquettes deliberately merge into a hand-composed area ("Error
handling" pulls from `panics` + `foreign_error_attenuation` +
`internal_error_chain` + two `antipatterns` rule ids). Some are
deliberately reference-only, non-actionable inventory (`error_sites`,
`error_chain`). The real invariant: **every registered quality etiquette
must explicitly declare its rollup fate — a dedicated area, or a
documented reason it declines one — never default to silently absent.**

## Design

- `QualityReportArea` trait (`src/etiquette.rs`): `fn quality_area(&self)
  -> Option<QualityAreaSpec>`. `None` is a real, valid, but *required*
  answer.
- `QualityAreaSpec`: title, checklist filename, summary filename, and a
  `compute: fn(&[&dyn Finding]) -> (usize, String)` function pointer
  owned by the etiquette itself.
- `StaticQualityEtiquette`: composition over `StaticEtiquette` plus a
  mandatory `quality_area` field — no `Default` impl, so a struct literal
  missing the field is a compile error, not a silent gap.
- `QualityEtiquette: Etiquette + QualityReportArea` marker supertrait
  (blanket-impl'd) — the type every quality etiquette module's static
  instance satisfies.
- One canonical registry, `etiquettes::quality_report_etiquettes() ->
  Vec<&'static dyn QualityEtiquette>`. Both `quality_etiquettes()`
  (session/plugin registration, existing public API, unchanged
  signature) and `quality_report_areas()` (consumed by
  `build_quality_report`) derive from this single list via trait-object
  upcasting. An etiquette missing from this one list doesn't run at all
  — there is no path to registering an etiquette for session use while
  it stays invisible to the rollup, or vice versa.
- Coverage etiquettes (`impl_coverage`, `trenchcoat`, `shadow`,
  `homecoming_std`, `amenable_std`) stay on plain `StaticEtiquette` --
  they feed a different report (`coverage-summary.md`) entirely and
  never touch this trait.

## What moved where

Each etiquette's own open-item counting/formatting logic (previously
centralized in `reporter/quality_report/metrics.rs`, reaching into every
etiquette's own rule-id namespace from outside) now lives with its
owning module, either inline in `mod.rs` or a small `quality_area.rs`
submodule for richer per-role/per-kind breakdowns (`tracing`,
`modularity`). `metrics.rs` keeps only `error_handling_metrics`/
`panic_metrics` — the one deliberately hand-composed "Error handling"
area, which by its own nature can't be any single etiquette's own
contribution.

## Status

| Task | Detail |
| --- | --- |
| Core trait/struct/registry | done |
| Migrate all 18 quality etiquette modules | done |
| Close the dormant `foreign_error_types` gap (new area) | done |
| `build_quality_report` rewritten to iterate the registry | done |
| Tests updated for the new 14-area order | done |

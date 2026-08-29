# Etiquette `explain`

`rustc --explain` / `clippy explain` work because a diagnostic is a code
and the explanation is what you read before you allow or deny. Cordial
findings look the same (`DOC-WARNING-001`, `doc_warnings`) and then dump
you into module rustdoc or `docs/planning/`.

The missing piece is not more policy text — every etiquette already has
What / Why / How in its module docs — it is a way to get that text from
a checklist id without opening the repo.

## The invariant

Every [`Etiquette`](../../src/etiquette.rs) must answer `explain()` —
why it exists, what it flags and ignores, how to turn it down. No
default method, so a new bundle that forgets is a compile error, not a
silent `TODO`. Same force as [`QualityReportArea`](quality-report-feeder-trait.md)
on `StaticQualityEtiquette`.

`StaticEtiquette` carries a mandatory `explain: EtiquetteExplain` field
and does **not** implement `Default`. Coverage etiquettes stay on
`StaticEtiquette` and still have to fill it.

## What the page is for

A user staring at an open checklist. Not implementer tables (IR keys,
scan status). Those stay in `docs/planning/`.

| Section | Content |
| --- | --- |
| Summary | One line for `cordial explain` with no argument |
| Why | Why the check exists |
| Logic | What is flagged, what is ignored, how the scan works |
| Opt out | The `cordial.toml` line that turns the etiquette off: `` `[doc_warnings] enabled = false` ``. Not exceptions (those suppress one site), not Cargo features. |
| Rules | Stable rule ids so `cordial explain DOC-WARNING-001` resolves to the same page |

Cordial is not rustc lint levels. There is no `allow(doc_warnings)` in
source. Every built-in table has `enabled = true` by default;
`enabled = false` skips that etiquette for the project. Per-site
exceptions stay a separate tool (`cordial exceptions add`) and do not
belong on the opt-out line.

## CLI

```text
cordial explain                 # id + one-line why, every etiquette in this binary
cordial explain doc_warnings    # full page
cordial explain DOC-WARNING-001 # same page (rule alias)
```

Lookup walks `all_plugins()` → unique etiquettes (plugins can share a
bundle). Unknown id is `CordialError::unknown_etiquette`. Custom plugins
are not on the CLI (no load path yet); they still implement `explain()`
so a session that registered them can print the page.

## Status

| Task | Detail |
| --- | --- |
| `Etiquette::explain` + `EtiquetteExplain` | done |
| `StaticEtiquette.explain` (no `Default`) | done |
| Built-in + example + test fixtures filled | done |
| `cordial explain [id]` | done |
| Tests | `tests/explain.rs`, `tests/cli.rs` |
| Opt-out is `[id] enabled = false` in cordial.toml | done |

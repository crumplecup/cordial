# `inline_tests` etiquette

Unit tests belong in the crate’s `tests/` tree, not under `src/` behind
`#[cfg(test)]`. Inline tests hide coverage from library readers and mix
test-only helpers into production modules.

This is cordial’s own layout rule (`CLAUDE.md`: tests in `tests/`, not
`#[cfg(test)]` in source). The etiquette makes that regeneratable for
any crate `cordial quality` scans.

---

## What counts

Scan **only** `src/` (library and binary sources). `tests/` is the
destination, not a finding.

| Shape | Rule |
| --- | --- |
| `#[cfg(test)] mod …` (inline or `mod foo;`) | `INLINE-TEST-MOD` |
| `#[cfg(test)]` on a non-mod item (fn, impl, use, …) | `INLINE-TEST-CFG` |
| `#[test]` / `#[tokio::test]` / last path segment `test` or `rstest`, **not** already inside a flagged `#[cfg(test)]` module | `INLINE-TEST-FN` |

Not flagged:

- `#[cfg(not(test))]`
- Anything under crate `tests/`
- `tests/fixtures` and `tests/parity` (skipped as elsewhere)

One finding per `#[cfg(test)]` module; inner `#[test]` fns are omitted so
the action is “move this module,” not a row per case.

Dogfood on cordial: 12 leftover `#[cfg(test)]` modules under `src/`. All
moved into `tests/`; cordial now reports **0** inline-test sites.

---

## Design

Syn visitor. Feature `inline_tests`, in `quality`. Same hook shape as
allows.

| Task | Detail |
| --- | --- |
| Scan + etiquette bundle | done |
| Tests in `tests/inline_tests_etiquette.rs` | done |

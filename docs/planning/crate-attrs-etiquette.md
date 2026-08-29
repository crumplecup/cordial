# `crate_attrs` etiquette

Crate-root inner attributes that sibling `CLAUDE.md` files require on every
library: `#![forbid(unsafe_code)]` and `#![warn(missing_docs)]`. Two easy
lints, one etiquette, one checklist grouped by workspace member.

---

## Problem

`homecoming` / `amenable` / `elicit_doc` all say:

- **Unsafe** — forbidden. Put `#![forbid(unsafe_code)]` in `lib.rs`.
- **Docs** — every public item, enforced by `#![warn(missing_docs)]`.

Those attributes only lock the crate when they sit on the **library root**.
A `src/lib.rs` that never declares them, or a `[lib] path = "src/core.rs"`
that people forget to check, leaves the whole crate open. Bin-only packages
have no library root; they are out of scope.

An FFI crate that *must* use `unsafe` should opt out of the forbid lint
without turning the rule off for every other member.

## Rules

| Rule | Fires when | Fix |
| --- | --- | --- |
| `CRATE-FORBID-UNSAFE-001` | The library root has no crate-level `forbid(unsafe_code)` | `#![forbid(unsafe_code)]` |
| `CRATE-MISSING-DOCS-001` | The library root has no crate-level `warn`/`deny`/`forbid(missing_docs)` | `#![warn(missing_docs)]` (stronger levels count) |

`deny(unsafe_code)` is **not** enough — later `#[allow(unsafe_code)]` can
undo it. `missing_docs` is allow-by-default and is not in the `warnings`
group, so `#![deny(warnings)]` does not count.

Crate-level `#![cfg_attr(..., forbid(unsafe_code))]` (and the same shape
for `missing_docs`) counts: the attribute still lives on the library root.

## Library root

Do not assume `src/lib.rs`.

1. If `Cargo.toml` has `[lib] path = "..."`, that file is the root
   (even when `src/lib.rs` also exists).
2. Else if `[lib]` is present without `path`, or `src/lib.rs` exists,
   use `src/lib.rs`.
3. Else the package has no library — skip both rules.

## Config

```toml
[crate_attrs]
# Both default true.
# forbid_unsafe = true
# missing_docs = true
# Package names (Cargo `[package].name`) exempt from that lint.
# allow_unsafe = ["ffi"]
# allow_missing_docs = []
```

Turning a bool off disables that rule workspace-wide. The allow lists are
the fine-grained hatch: keep forbid-unsafe on everywhere except the FFI
crate.

## Design

Same skeleton as `cfg_hygiene` (two rules, one category, checklist grouped
by each finding's own `crate` field so a workspace run lists every member
that is missing a declaration):

- `scan.rs` — resolve the library root, parse crate-level inner attrs.
- `types.rs` / enricher / probe / assessor / reporter.

Feature `crate_attrs = ["dep:toml"]` (manifest `[lib] path`), folded into
`quality`. Artifacts: `crate-attrs.checklist.md`, `crate-attrs.csv`,
`crate-attrs-summary.md`.

`cordial quality --apply` writes the missing inner attributes onto each
library root (same `[lib] path` / allow-list rules as the scanner).
`--dry-run` logs without writing. Tracing `--apply` still consumes
`tracing-instrument.checklist.md`; crate-attrs apply scans the tree and
does not need a prior `quality` run.

Turning on `#![warn(missing_docs)]` is the cheap half: every public item
then needs rustdoc. This crate's library root now carries both inner
attributes, and the public API is documented so `cargo check` is clean
under that lint. The dogfood test in `tests/crate_attrs_etiquette.rs`
asserts a clean scan of `cordial` itself.

## Status

| Task | Detail |
| --- | --- |
| Scan + `[lib] path` | done |
| Config toggles + per-crate allow lists | done |
| Checklist grouped by crate | done |
| Apply | `cordial quality --apply` inserts the missing inner attributes |
| Cordial dogfood | `src/lib.rs` has both attrs; public items documented |
| Tests | `tests/crate_attrs_etiquette.rs` |

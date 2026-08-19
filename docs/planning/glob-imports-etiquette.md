# `glob_imports` etiquette

Flag `use …::*` (and nested `*` in a use group). Glob imports hide the
names a file actually depends on, which breaks completion in most IDEs
and makes reviews, tracing recipes, and moves harder than an explicit
list.

Complements [modularity-etiquette.md](modularity-etiquette.md) (what belongs
in a file) and [tracing-etiquette.md](tracing-etiquette.md) (what a function
records).

---

## What counts

Every `*` in a `use` tree. No `super::*` carve-out: that shape is the
usual way child modules (including `#[cfg(test)]`) hide the parent
surface from completion.

| Shape | Flagged |
| --- | --- |
| `use foo::*;` | yes |
| `use foo::{bar, *};` | yes (the `*`) |
| `pub use foo::*;` | yes — re-export globs are the same opacity |
| `use crate::*;` / `use crate::foo::*;` | yes |
| `use super::*;` | yes — still a glob; still blinds IDE lookup |
| `use super::foo::*;` | yes |
| globs inside `#[cfg(test)]` modules | yes — same opacity, same completion cost |
| `use foo::{bar, baz};` | no |
| `use foo::Bar as Baz;` | no |

There is no percent knob: one remaining glob is a finding. Exception
patches silence a prelude or similar the crate truly wants.

Scan trees: crate `src/` and `tests/` (same as allows), skipping
`tests/fixtures` and `tests/parity`.

Dogfood on cordial: 22 sites (mostly `use super::*;` in cfg-gated child
modules and former inline tests). All replaced with explicit lists;
cordial now reports **0** open glob imports.

---

## Design

Syn visitor over `ItemUse` / `UseTree`. A `UseTree::Glob` is
`GLOB-IMPORT-001`. Snippet is the reconstructed path (`foo::*`).

Hooks: source loader, scope + inventory + attribute enrichers, probe,
assessor, CSV / checklist / summary reporters. Feature `glob_imports`, in
`quality`.

| Task | Detail |
| --- | --- |
| Scan + etiquette bundle | done |
| Tests in `tests/glob_imports_etiquette.rs` | done |

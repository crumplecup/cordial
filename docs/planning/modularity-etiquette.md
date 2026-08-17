# Modularity etiquette

A combined plugin for **modularity issues**: size, packing, and related
split/extract signals. Companion to [visibility](visibility-etiquette.md)
(module topology) and [cfg-scatter](cfg-scatter-etiquette.md) (feature-gate
scatter).

Thresholds live in `cordial.toml` under `[modularity]`. See
[cordial-config.md](cordial-config.md). Missing files fall back to
`ModularityThresholds::default`.

```toml
[modularity]
file_inventory_min_lines = 500
function_inventory_min_lines = 150
function_hotspot_min_lines = 80
file_checklist_min_lines = 1000
function_checklist_min_lines = 200
max_types_per_file = 10
module_size_sigma = 2
min_module_lines = 0
top_heavy_min_percent = 50
lopsided_min_percent = 75
hierarchy_min_lines = 150
```

## Rules

| Rule | When |
| --- | --- |
| `MODULARITY-FILE` | File line count ≥ `file_inventory_min_lines` (checklist at `file_checklist_min_lines`). |
| `MODULARITY-FUNCTION` | Function or method **body** line count ≥ `function_inventory_min_lines` (checklist at `function_checklist_min_lines`: split this body). On files already at `file_inventory_min_lines`, bodies ≥ `function_hotspot_min_lines` are also recorded so too-long hotspots can name extract-helpers; they are not CSV inventory. Free functions, inherent/trait impl methods, and trait default methods. Not signature-only trait items, not `#[cfg(test)]`. |
| `MODULARITY-TYPES-PER-FILE` | File-level type definitions exceed `max_types_per_file`. Always a checklist item. |
| `MODULARITY-MODULE-SIZE` | Every module is inventoried. Checklist iff its size is more than `module_size_sigma` sample standard deviations from the crate mean (`|z| > σ`, default 2). Two-tailed: unusually large *and* unusually small. Modules below `min_module_lines` are omitted from the sample (and never flagged). |
| `MODULARITY-TOP-HEAVY` | A parent (not the crate root) kept ≥ `top_heavy_min_percent` of its subtree in its own file, and own lines ≥ `hierarchy_min_lines`. Action: peel the leftover mass into children. |
| `MODULARITY-LOPSIDED` | One child holds ≥ `lopsided_min_percent` of its siblings' combined subtree after dropping siblings below `hierarchy_min_lines`, and at least two siblings remain. Action: split the dominant sibling. |

`max_types_per_file` is a packing cap, not a file-per-type rule. Default `10`
lets a handful of types share a file; only files above that become peel-types
actions. Counted types: `struct`, `enum`, `union`, and `trait` items in the
file, including types inside inline `mod` blocks. Not counted: type aliases,
impls, functions, or anything under `#[cfg(test)]`.

Function/method length is the **body** (the `{ ... }` block), not the
signature. Inventory starts at 150 body lines; checklist ("split this body")
starts at 200 so the action list names the methods that actually need
extracting. Too-long files also name bodies down to 80 lines as extract-helpers
without putting them in CSV inventory. Nested functions inside a long body can
fire on their own; closures are not counted. A too-long file with no body at
those floors still gets **extract helpers** — modularize often means peeling
predicates and shared constructors, not only growing a directory.

Module size: each `src/**/*.rs` file is a module (line count of that file),
plus each inline `mod { ... }` (span lines). `mod foo;` without a body is
not counted; `foo.rs` / `foo/mod.rs` is. `#[cfg(test)]` inline mods are
skipped. Stats are per crate (sample mean / stddev). n < 2 or σ = 0 means
no outliers, but the ranked list still appears in `modularity-summary.md`.

## Checklist composition

`modularity.checklist.md` is hotspot-oriented so FILE and MODULE-SIZE do
not appear as two unexplained rows for the same path:

- **Too long** — one item per oversized file/module, with the longest
  method bodies, packed-type list, and structure diagnosis nested on the
  same item:
  - split this body (checklist-length methods)
  - extract helpers from a named method, or from the file when no body
    reaches inventory (predicates, constructors, shared match arms)
  - peel types
  - or grow a subtree if those helpers form a named layer (fat leaf: no
    child modules yet)
  - peel the parent / split this dominant sibling when the same file is
    also a hierarchy hit
- **Split these bodies** — checklist functions that are not already nested
  under a too-long file.
- **Packed types** — files over `max_types_per_file` that are not already
  nested under a too-long file.
- **Rebalance** — top-heavy parents and lopsided dominant siblings that
  are not already nested under a too-long file.

## Hierarchy

File modules form a tree. Each node gets a **Horton–Strahler order** (leaves
are 1; a parent rises when two or more children share the max order) and a
**top-heaviness** score `own_lines / subtree_lines`. Rankings are per crate
so workspace graphs do not collide on `<crate>`.

Stream order stays diagnostic (mean own vs subtree by order). The other
structure views now have hit criteria and a named next action:

| Signal | Hit | Action |
| --- | --- | --- |
| Fat leaf | Too-long file with no child modules | Extract helpers first; grow a subtree only if those helpers belong together as a named layer |
| Top-heavy parent | Own/subtree ≥ 50% and own ≥ 150 | Peel remaining mass out of the parent into children |
| Lopsided siblings | One child ≥ 75% of sibling mass after dropping siblings below 150 lines | Split the dominant child |

`modularity-summary.md` ranks the same signals and marks Hits. Stream
order has no lint. `modularity-branches.csv` has every file-module node
(`crate` column first).

The CSV `lines` column is the measured magnitude: line count for size
rules, type count for `MODULARITY-TYPES-PER-FILE`, own lines for
top-heavy, dominant subtree for lopsided. Hierarchy rows also emit
`share` and `detail` (child or sibling masses).

## Status

Size rules, types-per-file (default 10), module-size 2σ, hotspot diagnosis
(including extract-helpers down to 80 body lines on too-long files), and
hierarchy lints (top-heavy peel, lopsided split at 75% after dropping stub
siblings) are in place. Modularize means extract helpers as well as split
into a directory.

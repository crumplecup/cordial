# Pageantry etiquette

How types are *arranged* in a file, not how many there are. Companion
to [modularity](modularity-etiquette.md) (packing / size) and
[visibility](visibility-etiquette.md) (module topology).

A file should read like a program: contracts first, then the types that
honor them. A trait that appears after types have already started is
ceremony in the wrong place — pageantry in the middle of the show.

---

## First rule: trait placement

Traits belong in one leading block, immediately below the header
(`use` / `extern crate` / `mod`). Several traits in a row at the top
are fine. A trait after a type (or any other body item) is not,
whether more types follow or the trait is last.

| Shape | Flagged |
| --- | --- |
| `use` / `mod`, then `trait A` / `trait B`, then structs | no — leading block |
| struct, struct, `trait`, struct | yes — sandwich |
| struct, struct, `trait` at EOF | yes — not at the top |
| `trait A`, struct, `trait B` | yes on `B` |
| `trait A`, `impl A for …`, `trait B` | yes on `B` — the block ended |
| file-level `mod foo;` before the trait block | no — header, like imports |
| `impl Trait for Type` (not a definition) | no |
| item under `#[cfg(test)]` | no — skipped, including the walk into that mod |
| trait in the middle of an inline `mod { … }` | yes — same rule, that module's item list |

Header items (`use`, `extern crate`, `mod`) never start the body, so a
typical `use` + `mod` + traits + types file stays clean. Everything
else (`struct` / `enum` / `union` / `type` / `fn` / `impl` / `const` /
`static` / `macro_rules!` / `extern "C"`) ends the leading trait
block. One finding per misplaced trait (`PAGEANTRY-TRAIT-001`).

Each file and each inline `mod { … }` is its own item list. Nested
content is not mixed into the parent walk.

```toml
[pageantry]
# enabled = true
# trait_block = true      # PAGEANTRY-TRAIT-001
# barrel = true           # PAGEANTRY-BARREL-001
# barrel_shim = true      # PAGEANTRY-BARREL-SHIM-001
# max_shim_lines = 8      # body lines allowed in a proc-macro entry point
```

## Design

Walk `File.items` (and inline mod contents) in source order. Classify
each item as header, trait (`Item::Trait` / `Item::TraitAlias`), or
body. After the first body item, every later trait is a record. Files
named `lib.rs` or `mod.rs` also record every non-header item as
`PAGEANTRY-BARREL-001`.

Hooks: source loader, scope + inventory + attribute enrichers, probe,
assessor, CSV / checklist / summary. Feature `pageantry`, in `quality`.

## Second rule: barrel files

`lib.rs` and `mod.rs` are the table of contents. They may declare
modules and re-export names. Types and functions belong in a named
sibling (`foo.rs`), not in the crate or directory index.

| Shape | Flagged |
| --- | --- |
| `mod inner;` + `pub use inner::Alpha;` in `lib.rs` | no — visibility and export |
| `pub fn helper()` in `lib.rs` or `mod.rs` | yes — `PAGEANTRY-BARREL-001` |
| `pub struct Alpha;` in `mod.rs` | yes |
| the same items in `foo.rs` | no — not a barrel filename |
| item under `#[cfg(test)]` | no — skipped |
| `#[proc_macro]` / `#[proc_macro_derive]` / `#[proc_macro_attribute]` fn in `lib.rs`, body within `max_shim_lines` | no — rustc requires these at a proc-macro crate root; a short delegating shim is fine |
| the same fn with a longer body | yes — `PAGEANTRY-BARREL-SHIM-001` |

The proc-macro exemption is by attribute, not by `Cargo.toml`: those
attributes only compile at a `proc-macro = true` crate root, so the scan
needs no crate-kind fact. The exemption is conditional on the entry point
staying a shim: the body is measured as the lines between its braces and
compared with `max_shim_lines` (default 8, set in `cordial.toml`). The
finding names the attribute, the function, the measured size, and the
limit, so the checklist row says what to do without a rule-id lookup. A
plain helper beside the entry points is still `PAGEANTRY-BARREL-001`.

Every rule has its own switch under `[pageantry]` so a project can opt out
of one without losing the others; the rule ids remain the handle for
`cordial exceptions`.

One finding per disallowed item. `use` / `extern crate` / `mod` are
legal; everything else in those two filenames is not. Later pageantry
rules (impl adjacency, inherent-before-foreign, …) share this
etiquette; they do not go into modularity.

| Task | Detail |
| --- | --- |
| `PAGEANTRY-TRAIT-001` scan + bundle | done |
| `PAGEANTRY-BARREL-001` scan + bundle | done |
| `PAGEANTRY-BARREL-SHIM-001` + `[pageantry]` knobs | done |
| Tests in `tests/pageantry_etiquette.rs` | done |
| Cordial dogfood | traits sit in a leading block; fat `mod.rs` files become named siblings |

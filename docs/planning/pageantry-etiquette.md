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
```

## Design

Walk `File.items` (and inline mod contents) in source order. Classify
each item as header, trait (`Item::Trait` / `Item::TraitAlias`), or
body. After the first body item, every later trait is a record.

Hooks: source loader, scope + inventory + attribute enrichers, probe,
assessor, CSV / checklist / summary. Feature `pageantry`, in `quality`.

Later pageantry rules (impl adjacency, inherent-before-foreign, …)
share this etiquette; they do not go into modularity.

| Task | Detail |
| --- | --- |
| `PAGEANTRY-TRAIT-001` scan + bundle | done |
| Tests in `tests/pageantry_etiquette.rs` | done |
| Cordial dogfood | traits sit in a leading block (`dogfood_cordial_traits_are_at_the_top`) |

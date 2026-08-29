# `doc_warnings` etiquette

`cargo check` / `clippy -D warnings` never run rustdoc. CI often does
(`RUSTDOCFLAGS=-D warnings cargo doc --no-deps`) and fails the build on
broken intra-doc links, invalid HTML, and the rest of the `rustdoc::*`
group. Easy to forget locally. Catch them in post: invoke `cargo doc`
and treat every rustdoc diagnostic as an action item.

Complements [crate_attrs](crate-attrs-etiquette.md) (does the library
root *declare* `#![warn(missing_docs)]`?). This etiquette is the
**compiler's output**, not attributes in source — the same split as
[verus_warnings](verus-warnings-etiquette.md).

---

## What counts

| Shape | Flagged |
| --- | --- |
| JSON `compiler-message` whose lint code starts with `rustdoc::` | yes — one finding per unique `(file, line, message)` |
| Human `warning[rustdoc::…]` / `error[rustdoc::…]` plus `--> file:line:col` | yes |
| rustc lints that happen to fire while rustdoc compiles (`unused`, `missing_docs`, …) | no — `cargo check` already sees those |
| `warning: N warning(s) emitted` | no — summary |
| `cargo` missing from `PATH` | no — skip the crate (quality must still run) |
| package listed in `[doc_warnings] skip_crates` | no |

Invocation (from the crate root):

```text
cargo doc --no-deps --message-format=json -p <package>
```

Optional flags from `cordial.toml`: `--all-features`,
`--document-private-items`. Override the binary with `CORDIAL_CARGO` or
`CARGO`. `--target-dir` is the crate's own `target/` so incremental
builds reuse rustc's cache.

---

## Config

```toml
[doc_warnings]
# enabled = true
# document_private_items = false
# all_features = true          # match CI that docs every feature
# skip_crates = ["proc-macro-helper"]
```

## Design

Parse cargo JSON (and rustc-style text as a fallback). Dedup because
rustdoc can reprint. Hooks: source loader, scope + inventory + attribute
enrichers, probe, assessor, CSV / checklist / summary. Feature
`doc_warnings`, in `quality`.

| Task | Detail |
| --- | --- |
| Scan + etiquette bundle | done |
| Tests in `tests/doc_warnings_etiquette.rs` | done |
| Cordial dogfood | `cargo doc --no-deps -p cordial` is clean of `rustdoc::*` (default features and `--features full`) |

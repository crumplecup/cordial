# cordial

[![License](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue.svg)](LICENSE-APACHE)

**Polite standards for code development.**

`cordial` is a plugin framework for local, regeneratable reports about whether a
codebase follows the etiquettes you care about. It refines
[`elicit_doc`](https://github.com/crumplecup/elicit_doc) with a trait-based
architecture: loaders, enrichers, probes, assessors, and reporters hook into a
shared graph IR so users can register custom lints without forking the tool.

Each **etiquette** is one polite standard. Quality etiquettes scan source.
Coverage etiquettes need rustdoc JSON. Artifacts land under
`~/.cordial/{project}/` (or `--store-home` / `CORDIAL_HOME`) and are never
committed to git.

Architecture: [CORDIAL_PLAN.md](CORDIAL_PLAN.md). Policy tables for individual
etiquettes: [PLANNING_INDEX.md](PLANNING_INDEX.md). Module-level docs (what /
why / how) live on each etiquette in `src/etiquettes/`.

## Install and run

```sh
cargo install --path .
# Quality-only binary: cargo install --path . --no-default-features --features quality,cli
```

```text
cordial quality -p <project>          # source-quality etiquettes
cordial quality --apply               # tracing recipes + crate-root lint attributes
cordial quality --apply --dry-run     # log apply without writing
cordial explain                       # id + one-line why for every compiled etiquette
cordial explain doc_warnings          # why it exists, what it flags, how to opt out
cordial explain DOC-WARNING-001       # same page (rule id alias)
# opt out: `[doc_warnings] enabled = false` in the project's cordial.toml
cordial build rustdoc                 # rustdoc JSON for coverage (needs elicitation)
cordial coverage                      # impl / trenchcoat / shadow (and std if enabled)
cordial run                           # quality + coverage
cordial exceptions list
cordial exceptions show <etiquette>
cordial exceptions add <etiquette> --file src/lib.rs --reason "..."
cordial exceptions load                  # copy repo registry into ~/.cordial
cordial exceptions backup                # copy store exceptions back into the repo
cordial view findings/rollup-summary.md
```

`-p` / `CORDIAL_PROJECT` selects the project root. `--crate-name` restricts a
run to one crate. `--store-home` / `CORDIAL_HOME` overrides `~/.cordial`.

## Store and config

Reports write to `{store_home}/{project}/findings/` (CSV inventories, markdown
checklists, summaries). A quality rollup lands at `findings/quality-report.md`.

Thresholds load later-wins from `CordialConfig::default`, then
`{store_home}/cordial.toml`, then `{workspace}/cordial.toml`. Missing files fall
back to defaults. Every etiquette table accepts `enabled` (default true);
`enabled = false` turns that lint off for the project. Canonical knobs:
committed [`cordial.toml`](cordial.toml) and
[docs/planning/cordial-config.md](docs/planning/cordial-config.md).

Hand-audited exceptions live in the target repo (CI) and copy into the
store before a run:

```sh
# From the target repo root, after cloning or cleaning ~/.cordial
cordial exceptions load
# After editing store patches, write them back for review
cordial exceptions backup
```

Default registry path is `{project}/.cordial-exceptions`. Relative paths
join the project root, so `cordial -p /repos/elicitation exceptions load
.elicit_doc-exceptions` works from any cwd. The slug-scoped tree is:

```text
.cordial-exceptions/
  {slug}/
    exceptions/          # quality suppressions: {etiquette}/{crate}.json
    quality/patches/     # elicit_doc alias for the same files
    patches/             # coverage skip lists: {crate}.json, {crate}-shadow.json
```

`load` replaces those store subtrees (stale files are deleted). Missing
`{root}/{slug}` is an error. `cordial exceptions list` / `show` inspect
what is in the store after load.

Create one store row at a time with flags (scripts / CSV loops):

```sh
cordial exceptions add panics --file src/lib.rs --rule-id PANIC-SOURCE-PANIC --reason "intentional"
cordial exceptions add panics --file src/lib.rs --line 12 --context demo::boom --reason "fixture"
cordial exceptions add --patch-set chrono --path chrono::DateTime --reason "upstream skip"
```

`--crate-name` selects the quality JSON stem (default: global `--crate-name`
or the project directory). Then `cordial exceptions backup` writes the
store back into the repo registry. The same append lives on the library
as `add_exception` / `add_coverage_skip`.

## Quality etiquettes

`cordial quality` runs every source-quality etiquette (Cargo feature `quality`,
included in the default `full` install). A quality-only binary:
`--no-default-features --features quality,cli`.

| Id | What it asks |
| --- | --- |
| `panics` | Where does this crate abort (`panic!`, `unwrap`, `expect`, `unreachable!`, `compile_error!`)? Libraries should return typed internal errors; binaries and tests should surface through miette. |
| `tracing` | Are functions instrumented with the recipe for their role? `cordial quality --apply` writes the recipe from the checklist. Volume is a subscriber `level` problem. `[tracing]` in `cordial.toml`. |
| `allows` | Which `#[allow]` / `#![allow]` attributes are in force? Verus `vstd` imports need `reason = "..."`. |
| `modularity` | Which files, functions, and modules are too large, overpacked, top-heavy, lopsided, or a unary nest? `[modularity]` in `cordial.toml`. |
| `derives` | Which manual builders, getters, setters, `new`, or pub fields could be derive crates? |
| `error_sites` | Where are `?`, `map_err`, and related error sites? Census for the layers below. |
| `error_chain` | Which converters drop `source()` instead of wrapping the original error? |
| `internal_error_chain` | Do this crate’s error types form the parent / boxed Kind / native-source architecture (location + `#[track_caller]` on sources)? |
| `foreign_error_types` | Which foreign `E` types leak onto this crate’s `Result` surface? |
| `foreign_error_attenuation` | How should those foreign sites be wrapped, mapped, or deferred? |
| `antipatterns` | Untyped carriers (`Box<dyn Error>`, `Result<_, String>`), unused `_arg`, static refs, unnamed contract bounds, version-in-member. |
| `cfg_scatter` | Is the same `#[cfg]` copied across item kinds instead of a gated `mod`? Field/variant gating is never flagged. `[cfg_scatter]` in `cordial.toml`. |
| `visibility` | Do `pub mod` paths earn their existence (flat crate, thin module, vis mismatch)? `[visibility]` in `cordial.toml`. |
| `cli_layout` | Do clap types live in the library and dispatch with `act`? Is `main` only parse + `act` + miette? |
| `crate_attrs` | Does each library root declare `#![forbid(unsafe_code)]` and `#![warn(missing_docs)]`? This crate does. `cordial quality --apply` writes them. `[crate_attrs]` in `cordial.toml` (per-member allow lists). |
| `doc_warnings` | Does `cargo doc` emit `rustdoc::*` diagnostics (broken intra-doc links, …) that `cargo check` never sees? This crate is clean. `[doc_warnings]` in `cordial.toml`. |
| `glob_imports` | Are there glob `use` trees (`foo::*`, including `super::*`)? Replace them with explicit names. |
| `inline_tests` | Are `#[cfg(test)]` modules or `#[test]` functions mixed into `src/`? Move them to `tests/`. |
| `verus_warnings` | Does the Verus rustc fork emit `warning:` diagnostics rustc never sees? Invokes `verus` on `*_verus` / `vstd` crates. |
| `proof_patterns` | Which `verus!` functions are trusted rather than proven (`assume`/`admit`/`external_body`/`uninterp`/`axiom`), or apply themselves invisibly to every proof in scope (`broadcast`)? |

Error-handling etiquettes share one source scan (`error_ir`). Quality `--apply`
rewrites source for tracing (`#[instrument]` recipes from the checklist) and
crate-attrs (crate-root lint attributes; scans the tree, no checklist).

## Coverage etiquettes

`cordial build rustdoc` (and `cordial build sysroot` for std-family), then
`cordial coverage`. The default `full` install includes `elicitation`
(impl / trenchcoat / shadow), `homecoming_std`, and `amenable_std`.

| Id | What it asks |
| --- | --- |
| `impl-coverage` | Do types implement the required elicitation traits (`ElicitComplete` and prerequisites)? |
| `trenchcoat` | Are foreign types wrapped before they reach those traits? |
| `shadow` | Do shadow crates mirror upstream items? |
| `homecoming-std` | How much of Rust `std` / `core` / `alloc` implements homecoming `Code`? |
| `amenable-std` | How much of that surface is in the amenable registry? |

## Library use

Register a built-in bundle or your own [`Etiquette`](src/etiquette.rs) on a
session. Plugins group related etiquettes; see
[docs/planning/custom-plugin-example.md](docs/planning/custom-plugin-example.md)
and `cargo run --example custom_plugins --features impl_coverage`.

```rust,ignore
use cordial::{PANICS_ETIQUETTE, SessionBuilder};

let session = SessionBuilder::new(project_root)
    .register(&PANICS_ETIQUETTE)
    .build();
```

Built-in plugins are feature-gated on the `cordial` crate (`panics`, `tracing`,
`quality`, `elicitation`, `full`, …). Crate-level feature list: `src/lib.rs`.

## Status

Phases 0–6 of [CORDIAL_PLAN.md](CORDIAL_PLAN.md) are on `main`. Output parity
with `elicit_doc` is retired -- see
[docs/planning/elicit-doc-parity.md](docs/planning/elicit-doc-parity.md) for
the historical record; cordial's own etiquettes now exceed what elicit_doc
could detect.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

# Planning index

Planning documents are design history and maintainer reference material. Current
user-facing guidance lives in `README.md`, `docs/running-cordial.md`,
`docs/writing-etiquettes.md`, and `docs/built-in-etiquettes.md`.

## Status vocabulary

| Status | Meaning |
| --- | --- |
| **Active** | Open work remains and the document should be updated as slices land. |
| **Reference** | The design is current enough to consult, but not an active task list. |
| **Complete** | The plan landed; keep for provenance unless it becomes misleading. |
| **Retired** | The direction is intentionally no longer pursued. |

## Current architecture

| Document | Status | Summary |
| --- | --- | --- |
| [CORDIAL_PLAN.md](CORDIAL_PLAN.md) | **Reference** | Architecture: etiquettes, graph IR, hook seams, build phases |
| [Documentation overhaul](docs/planning/documentation-overhaul.md) | **Complete** | Bottom-up doc rebuild landed: extension contracts, built-in etiquette tour, runtime/IR guides, README, archive triage |
| [cordial.toml config](docs/planning/cordial-config.md) | **Reference** | Layered config, built-in etiquette thresholds, and enabled gates |
| [Etiquette explain](docs/planning/etiquette-explain.md) | **Reference** | Required `Etiquette::explain`; `cordial explain [id]`; rule ids alias the page |
| [Custom plugin example](docs/planning/custom-plugin-example.md) | **Reference** | Downstream templates: `StaticPlugin`, `Coverage`, `ErrorHandling` |

## Plugin families

| Document | Status | Summary |
| --- | --- | --- |
| [Coverage as plugin](docs/planning/coverage-as-plugin.md) | **Reference** | Plugin / Coverage supertrait model; elicitation, homecoming, amenable profiles |
| [Error handling as plugin](docs/planning/error-handling-as-plugin.md) | **Reference** | Unified `ErrorHandling` plugin; parent / Kind / native-source architecture lints |
| [One crate, CLI in the library](docs/planning/one-crate-cli-layout.md) | **Reference** | One `CordialError`; `cli_layout` etiquette; clap dispatch in the library |

## Etiquette notes

| Document | Status | Summary |
| --- | --- | --- |
| [cfg_hygiene etiquette](docs/planning/cfg-hygiene-etiquette.md) | **Reference** | Static lint for misplaced verifier cfg names hidden by workspace-wide `--check-cfg` unions |
| [cfg_scatter etiquette](docs/planning/cfg-scatter-etiquette.md) | **Reference** | Static lint for `#[cfg(feature = "...")]` scattered across item kinds instead of module gates |
| [Crate attributes](docs/planning/crate-attrs-etiquette.md) | **Reference** | `#![forbid(unsafe_code)]` and `#![warn(missing_docs)]` on each library root |
| [Creusot diagnostics](docs/planning/creusot-diagnostics-etiquette.md) | **Reference** | Post-process `cargo creusot prove`; warnings and verifier failures become checklist items |
| [Derive patterns etiquette](docs/planning/derives-etiquette.md) | **Reference** | `derive_*` vs hand-rolled accessors; constructor arity to builder |
| [Doc warnings](docs/planning/doc-warnings-etiquette.md) | **Reference** | Post-process `cargo doc`; rustc/clippy never see `rustdoc::*` diagnostics |
| [Glob imports](docs/planning/glob-imports-etiquette.md) | **Reference** | Flag `use ...::*`; replace with explicit names |
| [Inline tests](docs/planning/inline-tests-etiquette.md) | **Reference** | `#[cfg(test)]` / `#[test]` under `src/` belong in `tests/` |
| [Modularity etiquette](docs/planning/modularity-etiquette.md) | **Reference** | Size, packing, hierarchy, file inventory, and extraction signals |
| [Pageantry etiquette](docs/planning/pageantry-etiquette.md) | **Reference** | File-level type arrangement; traits belong in a leading block |
| [Proof patterns etiquette](docs/planning/proof-patterns-etiquette.md) | **Reference** | `assume` / `admit` / `external_body` / `uninterp` / `axiom` / `broadcast` visibility |
| [Tracing etiquette](docs/planning/tracing-etiquette.md) | **Reference** | Classified instrumentation recipes, apply support, subscriber init, leftover stdio |
| [Verus compiler warnings](docs/planning/verus-warnings-etiquette.md) | **Reference** | Post-process `verus` output; rustc never sees these diagnostics |
| [Visibility etiquette](docs/planning/visibility-etiquette.md) | **Reference** | Public module topology; `prefer_root` and branching peel |

## Test-hardening notes

| Document | Status | Summary |
| --- | --- | --- |
| [Contract-bounds shape matrix](docs/planning/contract-bounds-shape-matrix.md) | **Reference** | Table-driven regression suite for `ANTIPATTERN-UNNAMED-CONTRACT-BOUND-001` clause matching |
| [Quality-report feeder trait](docs/planning/quality-report-feeder-trait.md) | **Complete** | `QualityReportArea` / `StaticQualityEtiquette` compiler-enforced rollup coverage |

## Completed and retired

| Document | Status | Summary |
| --- | --- | --- |
| [Post-parity alignment](docs/planning/post-parity-alignment.md) | **Complete** | R0-R7 migration complete; `collect/` removed; profile crates and shrunk public API |
| [IR enrichment](docs/planning/ir-enrichment.md) | **Complete** | Graph IR one-stop shop; inventory side caches retired |
| [elicit_doc parity](docs/planning/elicit-doc-parity.md) | **Retired** | Output parity vs elicit_doc baseline comparison tests is no longer chased |

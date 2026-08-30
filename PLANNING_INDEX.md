# Planning index

Active implementation plans for `cordial`.

| Document | Status | Summary |
| --- | --- | --- |
| [CORDIAL_PLAN.md](CORDIAL_PLAN.md) | **Active** | Architecture: etiquettes, graph IR, hook seams, build phases |
| [elicit_doc parity](docs/planning/elicit-doc-parity.md) | **Retired** | Output parity vs elicit_doc cache/reports; baseline comparison tests -- no longer chased, cordial's own etiquettes now exceed elicit_doc's coverage |
| [Coverage as plugin](docs/planning/coverage-as-plugin.md) | **Active** | Plugin / Coverage supertrait model; elicitation, homecoming, amenable profiles |
| [Post-parity alignment](docs/planning/post-parity-alignment.md) | **Complete** | Strangler map: elicit_doc straight ports extracted onto hook seams + IR (R0–R8) |
| [IR enrichment](docs/planning/ir-enrichment.md) | **Complete** | Graph IR one-stop shop; inventory side caches retired (I1–I5) |
| [Error handling as plugin](docs/planning/error-handling-as-plugin.md) | **Active** | Unified `ErrorHandling` plugin; parent / Kind / native-source architecture lints |
| [One crate, CLI in the library](docs/planning/one-crate-cli-layout.md) | **Active** | One `CordialError`; `cli_layout` etiquette; clap dispatch in the library |
| [cfg_scatter etiquette](docs/planning/cfg-scatter-etiquette.md) | **Active** | Static lint for `#[cfg(feature = "...")]` scattered across item kinds vs. mod-gated |
| [Crate attributes](docs/planning/crate-attrs-etiquette.md) | **Active** | `#![forbid(unsafe_code)]` and `#![warn(missing_docs)]` on each library root |
| [Doc warnings](docs/planning/doc-warnings-etiquette.md) | **Active** | Post-process `cargo doc`; rustc/clippy never see `rustdoc::*` diagnostics |
| [Custom plugin example](docs/planning/custom-plugin-example.md) | **Active** | Downstream templates: `StaticPlugin`, `Coverage`, `ErrorHandling` |
| [Derive patterns etiquette](docs/planning/derives-etiquette.md) | **Active** | `derive_*` vs hand-rolled accessors; constructor arity → builder |
| [Visibility etiquette](docs/planning/visibility-etiquette.md) | **Active** | `pub mod` paths must earn their existence; `prefer_root` vs branching peel |
| [cordial.toml config](docs/planning/cordial-config.md) | **Active** | Layered `cordial.toml` via the `config` crate; canonical etiquette thresholds |
| [Modularity etiquette](docs/planning/modularity-etiquette.md) | **Active** | Combined modularity plugin: size, packing, hierarchy lints |
| [Tracing etiquette](docs/planning/tracing-etiquette.md) | **Active** | Classify by role; recipe deltas; apply; attenuation; subscriber init policy |
| [Glob imports](docs/planning/glob-imports-etiquette.md) | **Active** | Flag `use …::*`; replace with explicit names |
| [Inline tests](docs/planning/inline-tests-etiquette.md) | **Active** | `#[cfg(test)]` / `#[test]` under `src/` belong in `tests/` |
| [Verus compiler warnings](docs/planning/verus-warnings-etiquette.md) | **Active** | Post-process `verus` output; rustc never sees these diagnostics |
| [Proof patterns etiquette](docs/planning/proof-patterns-etiquette.md) | **Active** | `assume`/`admit`/`external_body`/`uninterp`/`axiom`/`broadcast` via `verus_ir` |
| [Quality-report feeder trait](docs/planning/quality-report-feeder-trait.md) | **Complete** | `QualityReportArea`/`StaticQualityEtiquette` -- compiler-enforced rollup coverage, closes the `proof_patterns`/`foreign_error_types` silent-gap class |
| [Etiquette explain](docs/planning/etiquette-explain.md) | **Active** | Required `Etiquette::explain`; `cordial explain [id]` (rule ids alias the page) |
| [Pageantry etiquette](docs/planning/pageantry-etiquette.md) | **Active** | File-level type arrangement; first rule: traits in a leading block below the import/`mod` header |
| [Contract-bounds shape matrix](docs/planning/contract-bounds-shape-matrix.md) | **Active** | Table-driven `(verifier, shape, expected outcome)` regression suite for `ANTIPATTERN-UNNAMED-CONTRACT-BOUND-001`'s clause matcher; 13-row table implemented, growing |

## Completed / archived

| Document | Summary |
| --- | --- |
| [Post-parity alignment](docs/planning/post-parity-alignment.md) | R0–R7 migration complete; `collect/` removed; profile crates + shrunk public API |
| [elicit_doc parity](docs/planning/elicit-doc-parity.md) | Retired -- cordial's own etiquettes (e.g. the parent/Kind/native-source error architecture rules) now find real violations elicit_doc never could; there is no longer a frozen reference to stay in lockstep with. `tests/parity.rs`/`parity_refresh.rs`/`quality_parity.rs`/`coverage_parity.rs`/`coverage_parity_refresh.rs`/`elicitation_parity.rs`/`elicitation_parity_refresh.rs` and the frozen `tests/parity/baseline/` CSVs removed; `tests/parity/workspaces/` kept (shared fixtures for many ordinary etiquette tests, independent of baseline comparison). |

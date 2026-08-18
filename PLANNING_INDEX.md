# Planning index

Active implementation plans for `cordial`.

| Document | Status | Summary |
| --- | --- | --- |
| [CORDIAL_PLAN.md](CORDIAL_PLAN.md) | **Active** | Architecture: etiquettes, graph IR, hook seams, build phases |
| [elicit_doc parity](docs/planning/elicit-doc-parity.md) | **Active** | Output parity vs elicit_doc cache/reports; baseline comparison tests |
| [Coverage as plugin](docs/planning/coverage-as-plugin.md) | **Active** | Plugin / Coverage supertrait model; elicitation, homecoming, amenable profiles |
| [Post-parity alignment](docs/planning/post-parity-alignment.md) | **Complete** | Strangler map: elicit_doc straight ports extracted onto hook seams + IR (R0–R8) |
| [IR enrichment](docs/planning/ir-enrichment.md) | **Complete** | Graph IR one-stop shop; inventory side caches retired (I1–I5) |
| [Error handling as plugin](docs/planning/error-handling-as-plugin.md) | **Active** | Unified `ErrorHandling` plugin: panics + Result/chain stack; library vs miette surfaces |
| [cfg_scatter etiquette](docs/planning/cfg-scatter-etiquette.md) | **Active** | Static lint for `#[cfg(feature = "...")]` scattered across item kinds vs. mod-gated |
| [Custom plugin example](docs/planning/custom-plugin-example.md) | **Active** | Downstream templates: `StaticPlugin`, `Coverage`, `ErrorHandling` |
| [Visibility etiquette](docs/planning/visibility-etiquette.md) | **Active** | `pub mod` paths must earn their existence; `prefer_root` vs branching peel |
| [cordial.toml config](docs/planning/cordial-config.md) | **Active** | Layered `cordial.toml` via the `config` crate; canonical etiquette thresholds |
| [Modularity etiquette](docs/planning/modularity-etiquette.md) | **Active** | Combined modularity plugin: size, packing, hierarchy lints |
| [Tracing etiquette](docs/planning/tracing-etiquette.md) | **Active** | Classify by role; recipe deltas; apply writes the recipe; `[tracing]` config |

## Completed / archived

| Document | Summary |
| --- | --- |
| [Post-parity alignment](docs/planning/post-parity-alignment.md) | R0–R7 migration complete; `collect/` removed; profile crates + shrunk public API |

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

## Start here

```sh
cargo install --path .
cordial quality -p <project>
cordial explain
```

`cordial quality` runs source-quality etiquettes. `cordial coverage` runs
rustdoc-backed inventory etiquettes after `cordial build rustdoc`. Artifacts
land under `~/.cordial/{project}/findings/`.

## Reader paths

| Need | Read |
| --- | --- |
| Run the tool | [Running cordial](docs/running-cordial.md) |
| Understand built-in standards | [Built-in etiquettes](docs/built-in-etiquettes.md) |
| Build an etiquette or plugin | [Writing etiquettes](docs/writing-etiquettes.md) |
| Query the graph IR | [IR and queries](docs/ir-and-queries.md) |
| Review config knobs | [cordial.toml config](docs/planning/cordial-config.md) |
| Follow architecture | [CORDIAL_PLAN.md](CORDIAL_PLAN.md) |
| Track active plans | [PLANNING_INDEX.md](PLANNING_INDEX.md) |

Module-level docs for built-ins live on `src/etiquettes/*/mod.rs` and follow
the same What / Why / Flags / Ignores / Outputs / Config shape.

## Built-in standards

| Standard | Etiquettes |
| --- | --- |
| Error handling | `panics`, `error_sites`, `error_chain`, `internal_error_chain`, `foreign_error_types`, `foreign_error_attenuation` |
| Observability | `tracing`, `allows`, `doc_warnings`, `crate_attrs` |
| API shape | `visibility`, `derives`, `pageantry`, `glob_imports` |
| Structure | `modularity`, `inline_tests`, `cli_layout` |
| Conditional code | `cfg_scatter`, `cfg_hygiene` |
| Proof hygiene | `verus_warnings`, `proof_patterns`, contract-bound rules in `antipatterns` |
| Coverage | `impl-coverage`, `trenchcoat`, `shadow`, `homecoming-std`, `amenable-std` |

Run `cordial explain <id-or-rule-id>` for the compiled explanation of any
etiquette or rule in the current binary.

## Library use

Register a built-in bundle or your own [`Etiquette`](src/etiquette.rs) on a
session. Plugins group related etiquettes.

```rust,ignore
use cordial::{PANICS_ETIQUETTE, SessionBuilder};

let session = SessionBuilder::new(project_root)
    .register(&PANICS_ETIQUETTE)
    .build();
```

See [Writing etiquettes](docs/writing-etiquettes.md),
[IR and queries](docs/ir-and-queries.md),
[custom plugin examples](docs/planning/custom-plugin-example.md), and
`cargo run --example custom_plugins --features impl_coverage`.

## Status

Documentation is being rebuilt from the code outward. The active documentation
plan is [Documentation overhaul](docs/planning/documentation-overhaul.md).
Output parity with `elicit_doc` is retired; see
[elicit_doc parity](docs/planning/elicit-doc-parity.md) for the historical
record.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

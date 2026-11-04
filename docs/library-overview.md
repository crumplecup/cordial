`cordial` — polite standards for code development.

A plugin framework for local, regeneratable lint and coverage reports.
Each **etiquette** is a named bundle of loaders, enrichers, probes,
assessors, and reporters. Quality etiquettes scan source; coverage
etiquettes need rustdoc JSON. The `cordial` CLI writes artifacts under
`~/.cordial/{project}/`.

Start with the README for commands and reader paths. The built-in standards
tour explains what each etiquette enforces, the etiquette-author guide covers
extension traits, and module docs under `src/etiquettes/` document local
lint behavior. Architecture:
[CORDIAL_PLAN.md](https://github.com/crumplecup/cordial/blob/main/CORDIAL_PLAN.md).

# Features

Built-in plugins are feature-gated:

- `panics`, `tracing`, `allows`, `modularity`, `derives`, `error_sites`, `error_chain`, `internal_error_chain`, `foreign_error_types`, `foreign_error_attenuation`, `antipatterns`, `cfg_scatter`, `visibility`, `cli_layout`, `crate_attrs`, `doc_warnings`, `feature_warnings`, `glob_imports`, `inline_tests`, `verus_warnings`, `creusot_diagnostics`, `dependency_freshness`, `proof_patterns`, `pageantry` — source-quality scanners
  (the `quality` umbrella)
- `cli` — clap binary (`cordial`)
- `impl_coverage`, `trenchcoat`, `shadow` — rustdoc coverage scanners
- `elicitation` — umbrella for all coverage plugins
- `homecoming_std`, `amenable_std` — std-family coverage
- `full` — every built-in plugin; this is the default (`cargo install --path .`)

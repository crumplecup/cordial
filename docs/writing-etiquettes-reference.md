# Writing Etiquettes: Reference

*A companion to the series [Your First Etiquette](writing-etiquettes.md). The
series builds one lint step by step. This page collects the pieces it didn't
need.*

## The pipeline

```text
Loader -> IrEnricher -> Probe -> Assessor -> Reporter
                      \-> WorkspaceAssessor
```

The session deduplicates registered hooks by id, builds the IR once per active
crate, runs probes, routes markers to assessors, then writes reporter artifacts
under the project store.

| Hook | Job | Emits |
| --- | --- | --- |
| `Loader` | read raw material for one crate | `LoadView` |
| `IrEnricher` | add graph nodes, edges, and attributes | IR facts |
| `Probe` | find observations in the IR | `Marker` |
| `Assessor` | turn markers into judged issues | `Finding` |
| `WorkspaceAssessor` | judge cross-crate workspace facts | `Finding` |
| `Reporter` | render findings | `Artifact` |

`WorkspaceAssessor` is an optional extra the series didn't need. It judges facts
that span crates in a workspace.

## Choosing a plugin shape

The series used `StaticPlugin`. Three other shapes exist for other kinds of
product:

| Shape | Use when | Copy from |
| --- | --- | --- |
| `StaticPlugin` | a quality product contributes one or more ordinary etiquettes | `examples/custom_plugins/quality.rs` |
| `StrategicPlugin` | a product exposes alternate etiquette portfolios selected by a strategy enum | `tests/custom_plugins.rs` |
| `Coverage` | a product asks trait-implementation coverage questions over targets | `examples/custom_plugins/coverage.rs` |
| `ErrorHandling` | a product configures the error-flow analysis family | `examples/custom_plugins/error_handling.rs` |

A `StrategicPlugin` suits a plugin family with legitimate policy variants. The
strategy is usually a small enum parsed from `cordial.toml`, and each variant
returns the etiquette slice that should run. Keep ordinary plugins on
`StaticPlugin` until they need that choice.

For coverage etiquettes, use `StaticEtiquette` directly and set `is_coverage`
to `true`. Coverage reports do not feed `quality-report.md`.

`SessionBuilder::register(&ETIQUETTE)` is still handy for tests and small
one-off integrations. Prefer `register_plugin` for anything users should think
of as a product.

## Stable identifiers

Keep identifiers stable. Plugin ids, etiquette ids, hook ids, marker labels,
rule ids, and artifact filenames are all referred to by name from reports,
exceptions, and command-line filters, so renaming one breaks whatever points at
it.

## Going further

Read [Built-in etiquettes](built-in-etiquettes.md) for a tour of what ships with
cordial. Each etiquette module under `src/etiquettes/` documents its local What,
Why, and How to use, and `cordial explain` prints the compiled-in explanation for
the current binary.

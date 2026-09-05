# Writing etiquettes

An etiquette is a named standard implemented as a bundle of hooks. A plugin is
the product you register with a session; it contributes one or more etiquettes.

Use an etiquette when you need to load code, add facts to the IR, mark
interesting sites, judge those markers, and render artifacts. Use a plugin when
you want to ship one or more etiquettes under a single product id.

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

Markers are not findings. A probe should say "this site exists." An assessor
should say "this site violates rule X" or "this site is suppressed." A reporter
should only format already-judged findings.

For graph access patterns, read [IR and queries](ir-and-queries.md). That guide
covers `IrView`, `IrMut`, `QueryBuilder`, custom `Query` implementations, and
attribute conventions.

## Start with a static etiquette

Most source-quality checks can use `StaticEtiquette` or
`StaticQualityEtiquette`. Static declarations make the product shape visible at
the call site and keep missing documentation noisy.

```rust,ignore
use cordial::{
    EtiquetteExplain, EtiquetteHooks, EtiquetteRuleExplain, QualityAreaSpec,
    StaticEtiquette, StaticQualityEtiquette,
};

pub static ACME_STYLE: StaticQualityEtiquette = StaticQualityEtiquette::new(
    StaticEtiquette::new(
        "acme-style",
        "ACME style",
        EtiquetteHooks::new(
            &[&SOURCE_LOADER],
            &[&ACME_ENRICHER],
            &[&ACME_PROBE],
            &[&ACME_ASSESSOR],
            None,
            &[&ACME_REPORTER],
        ),
        false,
        EtiquetteExplain::new(
            "ACME-specific source checks.",
            "The project has local conventions that rustc and Clippy do not know.",
            "Scans source IR for ACME markers and emits findings for open violations.",
            "`[acme-style] enabled = false` in `cordial.toml`.",
            &[EtiquetteRuleExplain::new("ACME-STYLE-001", "Example ACME rule.")],
        ),
    ),
    Some(QualityAreaSpec::new(
        "ACME style",
        "acme-style.checklist.md",
        "acme-style-summary.md",
        count_open_findings,
    )),
);
```

For coverage etiquettes, use `StaticEtiquette` directly and set `is_coverage`
to `true`. Coverage reports do not feed `quality-report.md`.

## Choose the plugin shape

There are three supported product shapes:

| Shape | Use when | Copy from |
| --- | --- | --- |
| `StaticPlugin` | a quality product contributes one or more ordinary etiquettes | `examples/custom_plugins/quality.rs` |
| `Coverage` | a product asks trait-implementation coverage questions over targets | `examples/custom_plugins/coverage.rs` |
| `ErrorHandling` | a product configures the error-flow analysis family | `examples/custom_plugins/error_handling.rs` |

Register plugins, not individual hooks:

```rust,ignore
use cordial::{RunAll, Session, SessionBuilder};

let session = SessionBuilder::new(project_root)
    .register_plugin(&ACME_STYLE_PLUGIN)
    .build();

let outcome = session.run(&RunAll)?;
```

Direct `SessionBuilder::register(&ETIQUETTE)` is still useful for tests and
small one-off integrations. Prefer `register_plugin` for anything users should
think of as a product.

## Interface obligations

Keep identifiers stable. `Plugin::id`, `Etiquette::id`, hook ids, marker
labels, rule ids, and artifact filenames are part of the replay surface for
reports, exceptions, and command-line filters.

Return empty slices when a phase is not needed. A source-only lint may not need
a workspace assessor. A reporter-only summary may not need probes of its own.
The absence should be explicit in the static hook table.

Make `EtiquetteExplain` useful. It is the source for `cordial explain`, so it
should describe why the check exists, what it flags, how the scan behaves, and
how a project opts out.

For quality etiquettes, decide the rollup behavior. `Some(QualityAreaSpec)`
adds a row to `quality-report.md`; `None` is valid only when the etiquette is a
reference inventory or its findings are intentionally folded into another area.

## Built-in standards

The built-in etiquettes are easiest to understand by standard:

| Standard | Etiquettes |
| --- | --- |
| Error handling | `panics`, `error_sites`, `error_chain`, `internal_error_chain`, `foreign_error_types`, `foreign_error_attenuation` |
| Observability | `tracing`, `allows`, `doc_warnings`, `crate_attrs` |
| API shape | `visibility`, `derives`, `pageantry`, `glob_imports` |
| Structure | `modularity`, `inline_tests`, `cli_layout` |
| Conditional code | `cfg_scatter`, `cfg_hygiene` |
| Proof hygiene | `verus_warnings`, `creusot_diagnostics`, `proof_patterns`, contract-bound rules in `antipatterns` |
| Coverage | `impl-coverage`, `trenchcoat`, `shadow`, `homecoming-std`, `amenable-std` |

Read [Built-in etiquettes](built-in-etiquettes.md) for the tour. Each etiquette
module under `src/etiquettes/` documents its local What, Why, and How to use.
`cordial explain` prints the compiled-in explanation for the current binary.

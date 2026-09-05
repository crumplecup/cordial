# Documentation overhaul

`cordial` has enough live behavior now that the old plan-first documentation
shape is no longer the right reader path. The docs should be rebuilt from the
code outward: public method contracts, module-level architecture, etiquette
tours, then the README.

## Goal

Make the documentation explain both halves of the project:

- how to build and register etiquettes and plugins;
- how the built-in etiquettes enforce a broader Rust coding standard.

Planning documents remain useful provenance, but current user guidance should
come from the code contracts and the generated/static etiquette metadata, not
from stale phase notes.

## Source order

1. **Public method docs** — document obligations on `Etiquette`, `Plugin`,
   hook traits, object traits, `Coverage`, `ErrorHandling`, `Session`, and the
   static constructors. Each method should say what must be stable, what may be
   empty, and what the session does with the value.
2. **Core module docs** — make `etiquette`, `plugin`, `hooks`, `session`, `ir`,
   `objects`, and `reporter` form the extension-author API manual.
3. **Etiquette module docs** — every `src/etiquettes/*/mod.rs` should use the
   same shape: What, Why, What it flags, What it ignores, Outputs, Config, and
   Opt out.
4. **Reader guides** — add curated docs under `docs/` for writing an etiquette,
   writing a plugin, and reading cordial's built-in standards.
5. **README** — shrink to installation, common commands, concept map, and links
   to the deeper guides.
6. **Planning archive** — keep planning docs as design history, but mark which
   are current reference material and which are historical.

## Reader paths

| Reader | First page | Next page |
| --- | --- | --- |
| New user | `README.md` | `docs/built-in-etiquettes.md` |
| Lint author | `docs/writing-etiquettes.md` | `examples/custom_plugins/` |
| Plugin author | `docs/writing-etiquettes.md` | `docs/planning/custom-plugin-example.md` |
| Maintainer | `CORDIAL_PLAN.md` | this plan + current module docs |

## Etiquette tour groups

The built-ins should be introduced as standards, not alphabetical modules:

| Group | Etiquettes | Standard |
| --- | --- | --- |
| Error handling | `panics`, `error_sites`, `error_chain`, `internal_error_chain`, `foreign_error_types`, `foreign_error_attenuation` | library failures return typed crate errors, binaries/tests report through miette, foreign causes stay inspectable |
| Observability | `tracing`, `allows`, `doc_warnings`, `crate_attrs` | important work has spans, suppressions are reviewable, docs are compiled, unsafe and missing-docs policy is explicit |
| API shape | `visibility`, `derives`, `pageantry`, `glob_imports` | public paths, boilerplate, trait placement, and imports make contracts easy to see |
| Structure | `modularity`, `inline_tests`, `cli_layout` | files stay navigable, tests live in `tests/`, CLI parsing dispatches through library code |
| Conditional code | `cfg_scatter`, `cfg_hygiene` | gates are declared, local to the right verifier, and concentrated at module boundaries |
| Proof hygiene | `verus_warnings`, `creusot_diagnostics`, `proof_patterns`, contract-bound checks inside `antipatterns` | verifier-only warnings, Creusot prove failures, and trusted proof shortcuts are visible |
| Coverage | `impl-coverage`, `trenchcoat`, `shadow`, `homecoming-std`, `amenable-std` | trait coverage and adapter completeness are inventory questions, not source lints |

## Mechanical guardrails

The docs should reuse existing code-enforced metadata where possible:

- `Etiquette::explain` is mandatory, so `cordial explain` is the canonical
  short description, rule alias, logic, and opt-out source.
- `StaticQualityEtiquette` requires an explicit `QualityAreaSpec` or an
  intentional `None`, so the quality rollup cannot silently miss a new quality
  etiquette.
- `quality_etiquettes()` and `coverage_plugins()` are the built-in catalog for
  the current feature set; duplicated tables should either be derived from that
  shape or kept intentionally high-level.

## First slice

The first useful slice was intentionally small:

- tighten public docs on the extension traits and hook traits;
- add `docs/writing-etiquettes.md`;
- link that guide from the README and planning index.

That establishes the vocabulary before the larger etiquette-tour rewrite.

## Second slice

The second slice adds the standards tour:

- add `docs/built-in-etiquettes.md`;
- organize the built-ins by coding standard instead of module name;
- use `cordial explain` rule ids as the source for the tables;
- link the tour from the README and etiquette-author guide.

## Third slice

The third slice normalizes built-in module docs:

- each `src/etiquettes/*/mod.rs` entrypoint now follows the same What / Why /
  Flags / Ignores / Outputs / Config shape;
- tracing sub-etiquettes (`boundary`, `print`, `subscriber`) use the same
  shape as top-level etiquettes;
- `cargo doc --no-deps` is the guard for broken links and stale Rustdoc.

## Fourth slice

The fourth slice shrinks the README into an entry map:

- move day-to-day commands, store layout, config layering, and exception
  workflow into `docs/running-cordial.md`;
- keep `README.md` focused on concept, install, reader paths, standards map,
  library entrypoint, and status;
- preserve the detailed quality / coverage tour in `docs/built-in-etiquettes.md`
  rather than duplicating it in the README.

## Fifth slice

The fifth slice makes the core API modules read as an extension manual:

- add module docs for `session`, `hooks`, `ir`, `objects`, `reporter`, and
  `filter`;
- document `SessionView`, `RunFilter`, `RunOutcome`, `Session`, and
  `SessionBuilder` in terms of the actual runtime pipeline;
- clarify that filters select from already registered plugins and etiquettes,
  while sessions perform hook resolution, deduplication, target discovery,
  exception application, and artifact writing.

## Sixth slice

The sixth slice triages planning docs as an archive:

- replace the flat "active plans" list with explicit status vocabulary;
- split current architecture notes, plugin-family notes, etiquette notes,
  test-hardening notes, and completed/retired documents;
- add the missing `cfg_hygiene` planning note to the index;
- make clear that current user guidance now starts in `README.md`,
  `docs/running-cordial.md`, `docs/writing-etiquettes.md`, and
  `docs/built-in-etiquettes.md`.

## Seventh slice

The seventh slice adds concrete IR query guidance:

- tighten Rustdoc on `IrView`, `IrMut`, `NodeView`, `Query`, `QueryBuilder`,
  and core graph/index helpers;
- add `docs/ir-and-queries.md` with the IR mental model, read/query examples,
  custom query shape, enricher fact insertion, and attribute conventions;
- link the guide from the README and etiquette-author path.

## Eighth slice

The eighth slice closes the consistency loop:

- compare the standards tour against the compiled `cordial explain` catalog and
  add the missing `allows` etiquette to the public guide;
- refresh `README.md`, `docs/writing-etiquettes.md`, and `CORDIAL_PLAN.md` so
  the top-level summaries describe the current quality surface;
- check local Markdown links, the custom plugin example, generated Rustdoc, the
  full test suite, and the Clippy gate.

## Status

Complete. The bottom-up rebuild now covers extension contracts, core module
docs, built-in etiquette tours, runtime usage, IR query guidance, README entry
points, and planning-archive triage.

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
| [Strategy transformations](docs/planning/strategy-transformation.md) | **Active** | Construct selected designs through shared-IR apply: start from panic, generate typed error structure and caller signatures, and validate through executable Cargo examples |
| [Exceptions remove / edit](docs/planning/exceptions-remove-edit.md) | **Active** | `cordial exceptions remove` / `edit` with unique selectors; stale-exception warnings on run |
| [Agent worklist](docs/planning/agent-worklist.md) | **Active** | `cordial open` / `done` / `skip` / `submit`, with `peek` / `stats` / `instructions` beside them: a fixed worklist with a cursor the agent moves without re-running the gate; `submit` runs the gate and scores the batch |
| [Etiquette API and pipeline review](docs/planning/etiquette-api-review.md) | **Active** | Ordered review checklist: caller-location correctness, repeated source work, probe contract, typed plugin facts, tutorial corrections, and registration ergonomics |
| [Gate tiers](docs/planning/gate-tiers.md) | **Active** | Named cost tiers (`quick`, `build`, `deep`, plus user-defined) for etiquettes, partitioned from measured cold/warm timings; `--tier` selection |
| [One pass, one parse](docs/planning/one-pass.md) | **Active** | Fuse the per-file collectors so a file is parsed once whatever the cache capacity: one extraction query per file, projection queries over it, a priming pass in chunks; panics and antipatterns split into extract-then-filter |
| [Cold cache](docs/planning/cold-cache.md) | **Active** | Trade time for space when RAM is short: a pressure ladder, and for each cache type either a cold tier on disk or evict-and-recompute, so a run slows down instead of crashing |
| [Performance metrics](docs/planning/performance-metrics.md) | **Active** | Repeatable cost measurement: cordial-on-cordial workflow records (cold/warm/incremental) and a per-test profile that separates fixed cost from work; stored run records, counters that gate and times that trend, and a pre-merge diff against the baseline |
| [Memory measurement](docs/planning/memory-measurement.md) | **Active** | Consolidate allocation and retained-memory measurement through Hotpath; remove custom ledger/span attribution, cover process lifetimes and workers, and import every process report through the existing perf path |
| [Memory reduction](docs/planning/memory-reduction.md) | **Active** | Reduce the peak and the time of a run, measured through Hotpath and the run's own metrics: context retirement, per-file ownership, context scope, the save's verification, and the base-load decomposition of the small and medium workspaces |
| [Save, sharing and context cost](docs/planning/save-and-sharing.md) | **Active** | Plan: incremental save (segmented state, file texts by reference, dirty tracking), Salsa flattening (interval sets, one flatten per save), base layers for the sysroot, and cfg-context and leaf cost |
| [Definition-map sharing](docs/planning/definition-map-sharing.md) | **Active** | S0 to S5: a cfg copy takes the modules a gate does not reach from the original's definition map |
| [Definition map cells](docs/planning/definition-map-cells.md) | **Active** | M1 to M4: readers depend on a module's data and not on the whole definition map, so a flip runs again only what read what changed |
| [Contexts as flips](docs/planning/contexts-as-flips.md) | **Planned** | A context is a flip of the one base crate: salsa finds the delta, memory is base plus one delta; replaces the copy design (S1–S4) |
| [Incremental cache](docs/planning/incremental-cache.md) | **Active** | Replace the whole-run cache with Salsa-tracked facts so an edit recomputes only what it invalidates (a comment line reuses everything); a persistent session database across commands; reuse counters in the perf record |
| [rust-analyzer persistence](docs/planning/ra-persistence.md) | **Experiment** | Vendored `ra_ap_*` crates with persistable inputs, so a database saved by one process is restored by the next; per-crate reuse measured; item identity is the prerequisite for saving cordial's own facts |
| [Clean command](docs/planning/clean-command.md) | **Active** | `cordial clean` removes a project's regenerable store artifacts (cache, reports); `--all` covers every store and the shared sysroot cache; curated exceptions and config are never touched |

## Plugin families

| Document | Status | Summary |
| --- | --- | --- |
| [Coverage as plugin](docs/planning/coverage-as-plugin.md) | **Reference** | Plugin / Coverage supertrait model; elicitation, homecoming, amenable profiles |
| [amenable-ext-jiff coverage etiquette](docs/planning/amenable-ext-coverage-etiquette.md) | **Reference** | Plugin/probe/assessor/reporter architecture for third-party target crate registry coverage via shadow-dep rustdoc; mirrors amenable-std |
| [amenable-ext targets in cordial.toml](docs/planning/amenable-ext-targets-config.md) | **Complete** | Config-driven `[[amenable_ext.target]]` list, owned `Arc<dyn Etiquette>` registration. Steps 0-2 landed |
| [amenable-ext generic instantiations](docs/planning/amenable-ext-generic-instantiations.md) | **Active** | Per-instantiation coverage rows for generic types (chrono `DateTime`/`Date`), adapting to amenable's CHRONO_SUPPORT_PLAN; canonical type identity; aggregate parent rows. Phase 0 (registry read side) done |
| [Error handling as plugin](docs/planning/error-handling-as-plugin.md) | **Reference** | Unified `ErrorHandling` plugin; parent / Kind / native-source architecture lints |
| [Proc-macro crates terminate at syn::Error](docs/planning/error-chain-proc-macro-syn-error.md) | **Active** | Exempt `ERROR-CHAIN-COMPLIANCE-STRINGIFY-001`/`DISCARD-TYPED-001` when a proc-macro crate flattens a foreign error into `syn::Error::new`/`new_spanned` -- a real terminus, not a dropped chain |
| [Error field apply roadmap](docs/planning/error-field-apply-roadmap.md) | **Active** | Safe owned/borrowed `--apply` expansion: use-site guards, static expressions, constructor parameters, and private Kind/source graphs |
| [One crate, CLI in the library](docs/planning/one-crate-cli-layout.md) | **Reference** | One `CordialError`; `cli_layout` etiquette; clap dispatch in the library |

## Etiquette notes

| Document | Status | Summary |
| --- | --- | --- |
| [Antipatterns per-file skip](docs/planning/antipatterns-path-skip.md) | **Active** | `[antipatterns] skip_paths` keyed by rule id, reusing the `generated_files`/`mod_thin_skip`/`skip_folders` path-prefix idiom; motivated by Kani gallery fixtures that deliberately exercise the flagged shape |
| [cfg_hygiene etiquette](docs/planning/cfg-hygiene-etiquette.md) | **Reference** | Static lint for misplaced verifier cfg names hidden by workspace-wide `--check-cfg` unions |
| [cfg_scatter etiquette](docs/planning/cfg-scatter-etiquette.md) | **Reference** | Static lint for `#[cfg(feature = "...")]` scattered across item kinds instead of module gates |
| [Crate attributes](docs/planning/crate-attrs-etiquette.md) | **Reference** | `#![forbid(unsafe_code)]` and `#![warn(missing_docs)]` on each library root |
| [Creusot diagnostics](docs/planning/creusot-diagnostics-etiquette.md) | **Reference** | Post-process `cargo creusot prove`; warnings and verifier failures become checklist items |
| [Dependency freshness](docs/planning/dependency-freshness-etiquette.md) | **Active** | Survey `Cargo.toml` intent and `Cargo.lock` resolutions before manifest-policy, workspace-policy, and patch/minor/major freshness lints |
| [Derive patterns etiquette](docs/planning/derives-etiquette.md) | **Reference** | `derive_*` vs hand-rolled accessors; constructor arity to builder |
| [Doc warnings](docs/planning/doc-warnings-etiquette.md) | **Reference** | Post-process `cargo doc`; rustc/clippy never see `rustdoc::*` diagnostics |
| [Feature warnings](docs/planning/feature-warnings-etiquette.md) | **Active** | Run the feature powerset; warnings that fire only under some combinations become checklist items with a suggested `cfg` gate |
| [hotpath etiquette](docs/planning/hotpath-etiquette.md) | **Active** | Plan only: static coverage of `hotpath` instrumentation (every operation measured, one allocator, locks, channels and threads routed and named, storage sizes and query reuse reported, one guard); dogfood on cordial by hand first, then extract the scanners |
| [Glob imports](docs/planning/glob-imports-etiquette.md) | **Reference** | Flag `use ...::*`; replace with explicit names |
| [Inline tests](docs/planning/inline-tests-etiquette.md) | **Reference** | `#[cfg(test)]` / `#[test]` under `src/` belong in `tests/` |
| [Modularity etiquette](docs/planning/modularity-etiquette.md) | **Reference** | Size, packing, hierarchy, file inventory, and extraction signals |
| [Modularity nested-finding count mismatch](docs/planning/modularity-nested-finding-count-mismatch.md) | **Done** | Quality-area count now comes from the checklist renderer's top-level items; regression covers a file with both FILE and TYPES-PER-FILE findings |
| [Stringified error source chain gap](docs/planning/kind-variant-string-wrapper-loophole.md) | **Done** | Conversion-site stringification is checked without receiver-type inference; fanout defaults to zero as a separate policy |
| [Pageantry etiquette](docs/planning/pageantry-etiquette.md) | **Reference** | File-level type arrangement; traits belong in a leading block |
| [Proc-macro barrel](docs/planning/proc-macro-barrel.md) | **Done** | Crate kind in the IR, plus a configurable shim check (`PAGEANTRY-BARREL-SHIM-001`) for proc-macro entry points in `lib.rs` |
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

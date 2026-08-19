# Post-parity architectural alignment

Planning document for converging **internal implementation** onto the architecture
defined in [CORDIAL_PLAN.md](../../CORDIAL_PLAN.md) and
[coverage-as-plugin.md](coverage-as-plugin.md), now that
[elicit-doc-parity.md](elicit-doc-parity.md) has largely frozen **output**
expectations.

Parity gave us correct artifacts. Alignment gives us **one way to produce them** —
through trait seams (`Loader`, `IrEnricher`, `Probe`, `Assessor`, `Reporter`) and
graph IR, not parallel elicit_doc pipelines copied under new names.

Complements [CORDIAL_PLAN.md](../../CORDIAL_PLAN.md) phases 4–6 and coverage-as-plugin
phases C0–C7.

---

## Goal

Every analysis path should satisfy the three-act model:

```text
Read code into IR  →  Flag things of interest  →  Report
       ↑                        ↑                      ↑
   Load + Enrich            Probe + Assess          Reporter
```

**Success means:**

1. Coverage findings are traceable: **marker label → finding rule id → artifact row**.
2. Rustdoc JSON is loaded **once per cache key** per session (via `RustdocLoader`), not
   re-parsed inside assessors, reporters, or digest helpers.
3. `Coverage` plugins drive **target selection and gap semantics**; session does not
   hardcode etiquette id lists or bypass the hook graph.
4. Parity tests stay green — refactors change **structure**, not normalized outputs.

---

## Relationship to other plans

| Document | Role |
| --- | --- |
| [elicit-doc-parity.md](elicit-doc-parity.md) | **Behavioral contract** — what artifacts must match |
| [coverage-as-plugin.md](coverage-as-plugin.md) | **Registration model** — `Plugin`, `Coverage`, concrete profiles |
| [CORDIAL_PLAN.md](../../CORDIAL_PLAN.md) | **Hook seams + IR** — authoritative architecture |
| **This document** | **Migration map** — where straight ports live and how to extract hook patterns |

---

## The architectural contract (recap)

From CORDIAL_PLAN, each seam has a single job:

| Seam | Produces | Must not |
| --- | --- | --- |
| **Loader** | Opaque `LoadView` (source, rustdoc JSON, cargo metadata) | Emit findings or judgments |
| **IrEnricher** | Nodes, edges, attrs on the graph | Classify gaps or render CSV |
| **Probe** | **Markers** (observations) | Verdicts or file output |
| **Assessor** | **Findings** (judgments) | Re-load rustdoc or walk the filesystem ad hoc |
| **Reporter** | **Artifacts** (rendered files) | Orchestrate builds, diff inventories, or assess |

Coverage-specific vocabulary (`Coverage`, `TraitRequirement`, `CoverageTarget`,
`TargetProvider`, `GapContext`) lives at the **plugin** layer. Hooks remain
mechanical; plugins supply semantics.

### elicit_doc stage → cordial seam (target mapping)

| elicit_doc stage / module | Cordial target |
| --- | --- |
| Build rustdoc + cache keys | `src/cargo_rustdoc/` (unchanged infrastructure) |
| Inventory extract | `RustdocLoader` + `IrEnricher` (`TraitImpl`, `Trenchcoat`, `ShadowLink`) |
| Feature-probe rustdoc builds | `FeatureProbeEnricher` (attrs on type nodes) |
| Wrapper coverage map | `WrapperCoverageEnricher` or query over `Wraps` + `Implements` |
| Proof harness scan | `ProofHarnessEnricher` (attrs: `proof_test`, `composition_test`) |
| Impl gap assess | `ImplGapAssessor` reading `Implements` edges + enricher attrs |
| Shadow mirror diff | `WorkspaceAssessor` + `Mirrors` subgraph query |
| Framework std assess | Probe (std inventory scope) + Assessor (`Code` / registry) |
| CSV / checklist / summary | Reporters only |
| Post-run digest | Coverage plugin artifact or rollup reporter |

---

## Current state: hybrid architecture

Registration is ahead of execution. We have `Plugin`, `Coverage`, and concrete
profiles (`ElicitationCoverage`, `HomecomingStdCoverage`, `AmenableStdCoverage`),
but much **work** still follows elicit_doc's side-pipeline shape.

```text
                    ┌─────────────────────────────────────┐
  Intended path     │ Loader → Enrich → Probe → Assess → Report │
                    └─────────────────────────────────────┘
                                    ▲
                                    │ partial
                    ┌───────────────┴───────────────────────┐
  Straight port     │ collect/ → rustdoc diff → reporter orchestrates │
                    └─────────────────────────────────────────────┘
```

### Already on seams (keep and extend)

| Area | Evidence |
| --- | --- |
| Quality scanners | Panics, tracing, derives, error-sites, … — full hook loop |
| Trenchcoat etiquette | `TrenchcoatEnricher` + `UnwrappedForeignProbe` (anti-join on `Wraps`) |
| Impl probe | `MissingPrereqProbe` walks type nodes via `Query` |
| Same-crate shadow | `ShadowLinkEnricher` + `MissingShadowMirrorProbe` on `Mirrors` |
| Build / cache | `cordial build rustdoc`, shadow-dep keys, sysroot cache |
| Thin reporters (partial) | Impl/trenchcoat/shadow CSV reporters render from findings |

### Still elicit_doc-shaped (extraction targets)

| Smell | Where | What elicit_doc did |
| --- | --- | --- |
| **`src/collect/` module** | Whole tree | Parallel “inventory helpers” pipeline |
| **Assessor re-loads rustdoc** | `impl_coverage/assessor.rs` | Assessor calls `parse_rustdoc_json`, `collect::*` |
| **Reporter orchestrates** | `framework_std/*_reporter.rs` | Full `assess_*_std_coverage` in `render()` |
| **Cross-crate shadow bypasses IR** | `collect/shadow_pipeline.rs`, `shadow/report.rs` | Load two JSON inventories, diff in library code |
| **Session hardcodes coverage** | `session.rs` `COVERAGE_ETIQUETTE_IDS` | Monolithic pipeline family split by string ids |
| **Targets ignore `Coverage`** | `session.rs` `discover_crate_targets` only | `TRACKED_TARGETS` / roster not wired to session loop |
| **Digest side pipeline** | `digest/shadow_core_support.rs`, session L349+ | Post-run re-parse + rollup outside reporters |
| **Summary re-assesses** | `reporter/coverage_summary.rs` | Framework std assessed again for markdown section |

---

## Pattern catalog: extract hook patterns from straight ports

These are the **recurring extraction templates**. Each straight-port site should
map to one template.

### Pattern 1 — Side pipeline → Enricher

**When:** logic gathers **facts** (feature probes, wrapper map, proof harness,
dep features) that assessors need but that are not judgments.

**elicit_doc shape:** separate stage or `collect_*` helper called from assessor.

**Cordial shape:** `IrEnricher` runs once (workspace or per-crate), writes attrs or
edges; assessor reads `IrView` only.

**Examples:**

| Straight port | Enricher to introduce | IR effect |
| --- | --- | --- |
| `collect/feature_probe.rs` | `FeatureProbeEnricher` | `probed_prereqs` attr on type nodes |
| `collect/wrapper_coverage.rs` | `WrapperCoverageEnricher` | hub `Wraps`/`Implements` → cached map attr |
| `collect/proof_harness.rs` | `ProofHarnessEnricher` | `proof_test`, `composition_test` attrs |
| `collect/dep_features.rs` | `DepFeaturesEnricher` or `CargoLoader` | feature sets on dependency edges |

**Exit check:** assessor file has zero `parse_rustdoc_json` and zero `collect::` imports.

---

### Pattern 2 — Inventory diff → Probe + Assessor on IR

**When:** logic compares structured inventories (impl prereqs, shadow mirror rows).

**elicit_doc shape:** `build_*_report(inventory_a, inventory_b)` library functions.

**Cordial shape:**

1. Enrichers materialize inventory facts as graph nodes/edges/attrs.
2. **Probe** attaches markers (“missing prereq”, “shadow drift”, “unwrapped foreign”).
3. **Assessor** classifies markers into findings (`ImplGapKind`, `ShadowGapKind`).

**Examples:**

| Library today | Probe | Assessor | IR query |
| --- | --- | --- | --- |
| `MissingPrereqProbe` (done) | ✅ | `ImplGapAssessor` (partial) | `Implements` edges |
| `shadow/report.rs` same-crate | `MissingShadowMirrorProbe` (done) | `ShadowAssessor` | `Mirrors` |
| `collect/shadow_pipeline.rs` cross-crate | `ShadowPairScopeProbe` (scope only) | `CrossCrateShadowAssessor` ⚠️ still calls collect | needs **WorkspaceAssessor** |

**Exit check:** no `build_shadow_report_from_inventories` on the hot path; library kept
only for unit-test reference until IR path proven equivalent.

---

### Pattern 3 — Reporter orchestration → Assessor findings

**When:** `Reporter::render` calls `assess_*`, `build_*_report`, or `collect::*`.

**elicit_doc shape:** stage writes CSV directly after assess inline.

**Cordial shape:** assessor emits findings; reporter formats columns/checklist from
finding fields only.

**Examples:**

| Reporter | Orchestration today | Target |
| --- | --- | --- |
| `HomecomingStdReporter` | `assess_homecoming_std_coverage` | `FrameworkStdProbe` + `FrameworkStdAssessor` |
| `AmenableStdReporter` | `assess_amenable_std_coverage` | same + registry enricher |
| `ShadowMethodChecklistReporter` | `collect::build_shadow_pair_report` | method/trait diff as assessor findings |

**Exit check:** `render()` body contains no `assess_*`, `load_*`, `build_*_report`, or
`parse_rustdoc_json`.

---

### Pattern 4 — Session special case → Plugin metadata

**When:** session branches on hardcoded etiquette ids or duplicates plugin semantics.

**elicit_doc shape:** `run_coverage_all` knows every stage name.

**Cordial shape:**

- Target list from `Coverage::targets()` / `TargetProvider`
- Rollup/summary from plugin category or `Coverage` summary hook
- Enricher ordering from declarative priority, not string match in session

**Examples:**

| Session special case | Plugin-driven replacement |
| --- | --- |
| `COVERAGE_ETIQUETTE_IDS` | `PluginCategory::Coverage` |
| `discover_crate_targets` only | Union of active `Coverage` plugin targets |
| `build_coverage_summary` + digest in session | Coverage plugin post-run reporters |
| `select_load_view` for `"trait-impl" \| "trenchcoat"` | `IrEnricher::required_loader()` (new seam) |

---

### Pattern 5 — Post-run digest → Reporter artifact

**When:** session or `digest/` re-walks data after the hook loop.

**elicit_doc shape:** executive summary stage reads caches + findings.

**Cordial shape:** rollup **Reporter** or `Coverage` plugin emits digest artifact from
findings already in the run outcome (plus roster provider for static config).

**Example:** `digest/shadow_core_support.rs` → findings from impl/shadow reporters +
`TargetProvider` roster; no rustdoc re-load.

---

## Strangler map (file-level)

Priority key:

| Pri | Phase | Focus |
| --- | --- | --- |
| **A** | Session + plugin wiring | `Coverage::targets`, remove hardcoded ids |
| **B** | Framework std | Probe/assessor split from reporters |
| **C** | Elicitation collect → enrichers | Feature probe, wrapper map, ban assessor loads |
| **D** | Cross-crate shadow | WorkspaceAssessor + `Mirrors` |
| **E** | Summary + digest | No post-run side pipelines |
| **F** | CORDIAL_PLAN enrichers | Proof harness, PathIndex, SynDocLink, ErrorFlow |

### Master table

| # | Location | Key symbols | Port pattern | Target seam | Extraction step | Parity guard | Pri |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | `src/session.rs` | `COVERAGE_ETIQUETTE_IDS`, `run` | Hardcoded pipeline families | Plugin category | Derive coverage vs quality from registered plugins | `coverage_summary`, plugin registry | **A** |
| 2 | `src/session.rs` | `discover_crate_targets` | Ignores roster / shadow pairs | `Coverage::targets` | Union targets from active coverage plugins | `elicitation_parity`, plugin registry | **A** |
| 3 | `src/session.rs` | `select_load_view`, `enricher_order` | Stringly-typed hook wiring | Enricher metadata | `required_loader()`, `priority()` on enrichers | integration tests | **A** |
| 4 | `src/framework_std/run.rs` | `assess_homecoming_std_coverage` | Framework stage pipeline | Probe + Assessor | Split inventory walk vs impl match | `coverage_parity`, `sysroot_cache` | **B** |
| 5 | `src/framework_std/amenable.rs` | `assess_amenable_std_coverage` | Registry + std assess | Probe + Assessor + enricher | Registry dump → graph attrs | `coverage_parity`, `amenable_registry` | **B** |
| 6 | `src/etiquettes/framework_std/reporter.rs` | `HomecomingStdReporter::render` | Orchestrator in reporter | Reporter only | Findings from assessor | Tier C std CSV | **B** |
| 7 | `src/etiquettes/framework_std/amenable_reporter.rs` | `AmenableStdReporter::render` | Same | Reporter only | Same | Tier C amenable CSV | **B** |
| 8 | `src/collect/feature_probe.rs` | `load_crate_feature_probes` | Probe rustdoc rebuild | `FeatureProbeEnricher` | Attrs on type nodes | `feature_probe_gaps`, elicitation parity | **C** |
| 9 | `src/collect/wrapper_coverage.rs` | `load_workspace_wrapper_coverage` | Hub wrapper inventory | `WrapperCoverageEnricher` | Query `Wraps` on hub IR | `wrapper_coverage_gaps` | **C** |
| 10 | `src/etiquettes/impl_coverage/assessor.rs` | `ImplGapAssessor::assess`, `load_crate_inventory` | Assessor loads JSON | Assessor reads IR | Remove all direct loads; use enrichers | elicitation parity, impl gaps | **C** |
| 11 | `src/collect/dep_features.rs` | `collect_member_dep_build_config` | Cargo metadata stage | Loader / enricher | Dep features on graph | `build/shadow_dep` tests | **C** |
| 12 | `src/collect/shadow_pipeline.rs` | `build_shadow_pair_report` | Two-inventory diff | WorkspaceAssessor | IR subgraph compare | `shadow_pair_gaps`, elicitation parity | **D** |
| 13 | `src/shadow/report.rs` | `build_shadow_report*` | Inventory diff library | Probe + Assessor (via IR) | Keep as test oracle; retire hot path | shadow parity | **D** |
| 14 | `src/etiquettes/shadow/pair_assessor.rs` | `CrossCrateShadowAssessor` | Assessor runs collect | WorkspaceAssessor | Workspace pass after per-crate IR | shadow CSV/gaps | **D** |
| 15 | `src/etiquettes/shadow/reporter.rs` | `ShadowMethodChecklistReporter` | Reporter runs collect | Reporter only | Method/trait diff → findings | shadow checklist | **D** |
| 16 | `src/digest/shadow_core_support.rs` | `build_shadow_core_support_digest` | Post-run re-parse | Reporter / plugin hook | Build from findings + roster | `elicitation_coverage_summary` | **E** |
| 17 | `src/reporter/coverage_summary.rs` | `homecoming_std_section` | Re-runs assess | Plugin summary hook | Cache assessor output in session | `coverage_summary` | **E** |
| 18 | `src/reporter/elicitation_summary.rs` | `lookup_crate_version` | Rustdoc parse for version | Loader attr | Version on crate node at load | summary metrics | **E** |
| 19 | `src/collect/proof_harness.rs` | `load_workspace_proof_harness` | Hub test file scan | `ProofHarnessEnricher` | Type node attrs | `proof_harness_collect` | **F** |
| 20 | `src/collect/test_status.rs` | `test_status_for_type_path` | Helper (OK if enricher-fed) | Assessor input | Called only with enricher attrs | proof harness parity | **F** |

### `src/collect/` retirement criteria

Delete or shrink `collect/` when:

- [x] No etiquette, reporter, or session code imports `crate::collect::`
- [ ] `build/shadow_dep.rs` uses `CargoLoader` / dep enricher only
- [x] Public `lib.rs` exports for collect helpers removed or moved to proper modules (`feature_probe`, `build::dep_features`, `rustdoc::load_workspace_wrapper_coverage`)

Until then, **`collect/` was the canonical list of straight-port debt** — removed in R6; remaining build-time dep resolution in `build/dep_features.rs` is infrastructure, not hook bypass.

---

## What stays as library code

Not everything moves into hook structs. Shared **pure logic** remains in modules
but must sit **below** the seams:

| Module | Role after alignment |
| --- | --- |
| `src/rustdoc/` | Parse/normalize rustdoc JSON; called from `RustdocLoader` and build stage only |
| `src/cargo_rustdoc/` | Cache orchestration; not part of the analyze loop |
| `src/shadow/matching.rs`, `verification.rs` | Pure functions invoked from assessors |
| `src/etiquettes/impl_coverage/gap_classify.rs` | Gap taxonomy; invoked from assessor via `Coverage::classify_gap` |
| `src/plugin/coverage.rs` | Shared coverage vocabulary |

**Rule:** if a function needs `SessionView` and loads files, it belongs in a Loader,
Enricher, or (for cross-crate) WorkspaceAssessor — not a free-standing `collect` helper.

---

## Implementation phases

Each phase ends with parity tests green on Tier A; Tier C before declaring
framework/extraction complete.

### Phase R0 — Guardrails (this document)

- [x] Strangler map + pattern catalog
- [x] Listed in [PLANNING_INDEX.md](../../PLANNING_INDEX.md)
- [x] Cross-link from [coverage-as-plugin.md](coverage-as-plugin.md)

**Exit:** team agrees **no new coverage features via `collect/` or reporter orchestration**.

---

### Phase R1 — Session consumes plugins (A)

Wire the registration layer into execution.

| Task | Detail |
| --- | --- |
| `Coverage::targets` in session loop | Union member, upstream dep, shadow pair targets from active plugins |
| Remove `COVERAGE_ETIQUETTE_IDS` | Use `PluginCategory::Coverage` + `Etiquette::is_coverage` for summary branching |
| Enricher metadata | `priority()`, `required_loader()` — replace string matches in session |

- [x] `discover_run_crate_targets` unions active coverage plugin targets
- [x] Session uses plugin-driven targets instead of workspace-only discovery
- [x] `IrEnricher::priority` / `required_loader` replace hardcoded enricher wiring
- [x] `StaticEtiquette::is_coverage` marks coverage hook bundles
- [x] `ElicitationTargetProvider` resolves roster from full workspace before crate filter

**Exit:** `ElicitationTargetProvider` roster drives which crates run; shadow pairs schedule both upstream and shadow crate IR when filtered to upstream.

---

### Phase R2 — Framework std on hooks (B)

Prove the full loop on the simplest coverage profile (`Code` / registry).

| Task | Detail |
| --- | --- |
| [x] `FrameworkStdProbe` | Markers for std inventory types in scope |
| [x] `FrameworkStdAssessor` | `Code` / registry gap findings |
| [x] Thin `HomecomingStdReporter` / `AmenableStdReporter` | Render only |
| [x] Deprecate direct `assess_*_std_coverage` in reporters | Orchestration lives in assessor |

**Exit:** `framework_std/run.rs` is thin glue or removed; Tier C `coverage_parity` green.

**Status:** COMPLETED — probe marks std inventory on hub impl crate; assessor emits row
findings; reporters rebuild reports from findings. `assess_*_std_coverage` remains library
glue called from assessors only.

---

### Phase R3 — Elicitation enrichers (C)

Migrate `collect/` fact-gathering into enrichers; slim `ImplGapAssessor`.

| Task | Detail |
| --- | --- |
| [x] `FeatureProbeEnricher` | Replaces `load_crate_feature_probes` in assessor hot path |
| [x] `WrapperCoverageEnricher` | Replaces `load_workspace_wrapper_coverage` in assessor hot path |
| [x] Assessor reads IR only | `Implements` + enricher attrs; no rustdoc re-load |
| [ ] Loader policy | Single rustdoc load per cache key (probe rustdoc still separate cache) |

**Exit:** `impl_coverage/assessor.rs` has no `parse_rustdoc_json`; feature + wrapper parity green.

**Status:** COMPLETED (core) — `FeatureProbeEnricher` and `WrapperCoverageEnricher` wired into
`IMPL_COVERAGE_ETIQUETTE`; assessor reads `feature_probe_*` and `wrapper_coverage` attrs via
`node_context.rs`. Build helpers remain in `collect/` as enricher glue (same pattern as R2
`assess_*` in assessors). Proof harness still via `collect/` until R6.

---

### Phase R4 — Cross-crate shadow on IR (D)

Hardest elicitation slice; do after R3 proves enricher + assessor split.

| Task | Detail |
| --- | --- |
| [x] Introduce `WorkspaceAssessor` hook | Runs after per-crate probe/assess when shadow pairs active |
| [ ] Materialize upstream + shadow subgraphs | Via `Mirrors` + cross-crate IR (or workspace-scoped IR view) |
| [x] Retire `collect/shadow_pipeline.rs` hot path | Pair orchestration in `shadow/pair.rs`; `collect/` re-exports only |
| [x] Method/trait checklist | `ShadowMethodChecklistFinding` from workspace assessor; reporter renders findings |

**Exit:** shadow Tier A parity green without `build_shadow_pair_report` on hot path.

**Status:** COMPLETED (core) — `CrossCrateShadowWorkspaceAssessor` runs once per session;
per-crate `CrossCrateShadowAssessor` / `ShadowPairScopeProbe` removed. Pair diff still uses
`shadow/report.rs` inventory engine (IR subgraph compare deferred). `discover_active_shadow_pairs`
resolves roster from full workspace before crate filter.

---

### Phase R5 — Summary and digest (E)

| Task | Detail |
| --- | --- |
| Move `build_shadow_core_support_digest` | Coverage plugin reporter or rollup |
| `coverage_summary` | Read cached findings / session outcome, not re-assess framework std |
| Crate version in summary | From cargo/rustdoc loader attr, not ad hoc parse |

- [x] `homecoming_std_section` / `amenable_std_section` rebuild reports from assessor findings
- [x] `shadow-core-support.json` emitted from elicitation coverage rollup, not session
- [x] `RustdocLoader` sets `crate_version` on crate root; summary reads from workspace IR

**Exit:** session `run()` does not call `digest::` or `assess_*` directly.

**Status:** COMPLETED — `build_coverage_summary` takes workspace IR + findings; elicitation rollup
emits digest artifact; framework std summary sections use `*_report_from_findings`.

---

### Phase R6 — Remaining CORDIAL_PLAN enrichers (F)

| Enricher | Replaces |
| --- | --- |
| `ProofHarnessEnricher` | `collect/proof_harness.rs` |
| `PathIndexEnricher` | ad hoc path lookups |
| `SynDocLinkEnricher` | manual syn ↔ rustdoc joins (quality + coverage) |
| `ErrorFlowEnricher` | error-site multi-phase joins |

- [x] `ProofHarnessEnricher` — `proof_test` / `composition_test` attrs on type nodes; assessor reads IR only
- [x] Proof harness scan moved to `src/proof_harness/` (`collect/` re-exports shim pending full deletion)
- [x] `PathIndexEnricher` — rebuilds `by_path` index; session auto-injects before path-dependent enrichers
- [x] `SynDocLinkEnricher` — links syn/rustdoc item nodes by `qualified_path`; path index prefers rustdoc
- [x] `ErrorFlowEnricher` — partitions error-site nodes, sets origin/foreign attrs, materializes `ErrorFlow` edges; session auto-injects after error-site inventory

**Exit:** `collect/` deleted; CORDIAL_PLAN enricher table fully checked.

**Status:** COMPLETED — proof harness, path index, syn-doc link, and error flow on enricher seams; `collect/` removed (logic in `feature_probe/`, `build/dep_features`, `rustdoc/workspace_wrapper`).

---

### Phase R7 — Crate boundaries + public API

| Task | Detail |
| --- | --- |
| `cordial_elicitation` owns roster + `ElicitationCoverage` | Move `ELICITATION_TRACKED_TARGETS` behind provider |
| Optional `cordial_homecoming`, `cordial_amenable` | Thin profile crates |
| Shrink `lib.rs` exports | Traits, session, plugins — not diff engines |

- [x] `cordial_elicitation/src/tracked_targets.rs` — canonical roster; `cordial` includes via `plugin/elicitation_tracked_targets.rs`
- [x] `cordial_elicitation` re-exports `ElicitationCoverage`, target helpers, and shadow-core digest from `cordial` (impl stays in `cordial` to avoid circular deps)
- [x] `cordial_homecoming`, `cordial_amenable` — thin profile crates in workspace
- [x] Parity oracles moved to `#[doc(hidden)] cordial::testing` (shadow diff engines, framework std assessors, rustdoc parsers, impl-gap classifiers)
- [x] Public `lib.rs` no longer exports diff-engine helpers at crate root; shadow-dep **build** stays on `cordial::build`

**Exit:** downstream users register plugins; they do not import `build_shadow_*` or `collect::*`.

**Status:** COMPLETED — profile crates wired; roster owned by `cordial_elicitation`; root API shrunk; parity tests use `cordial::testing`.

### Phase R8 — Architectural fidelity gaps (post-R7)

- [x] Cross-crate shadow uses session-cached rustdoc inventories (`WorkspaceIr::rustdoc_inventories`); `CrossCrateShadowWorkspaceAssessor` no longer re-parses JSON on the hot path
- [x] Framework std assessors judge per-marker from IR `Implements` edges + skip/registry config (not `assess_*_std_coverage` orchestration)
- [x] Shadow-core digest reads findings + workspace inventory cache only (no `load_workspace_wrapper_coverage` / `parse_rustdoc_json` side loads)
- [x] Inventory diff helpers (`build_shadow_report_from_inventories`, …) confined to `cordial::testing` and legacy oracle entry points

**Status:** COMPLETED — hot paths follow hook loop; inventory diff is parity-oracle only.

---

## Suggested order (dependency-aware)

```text
R0 guardrails
  → R1 session/plugin wiring (unblocks correct target scheduling)
  → R2 framework std (proves Probe/Assessor/Reporter split end-to-end)
  → R3 elicitation enrichers (largest collect/ removal)
  → R4 cross-crate shadow (needs workspace pass + IR)
  → R5 summary/digest
  → R6 remaining enrichers
  → R7 crate boundaries
```

R2 before R3 is deliberate: homecoming/amenable have a **smaller denominator** and
already expose the “reporter-only etiquette” smell most clearly.

---

## Parity harness as refactor guard

| Tier | Tests | Protects |
| --- | --- | --- |
| **A** | `tests/elicitation_parity.rs`, `tests/shadow_pair_gaps.rs`, `tests/feature_probe_gaps.rs`, `tests/wrapper_coverage_gaps.rs`, `tests/proof_harness_collect.rs` | Elicitation coverage rows + gaps |
| **A** | `tests/coverage_etiquette.rs` | Plugin registration + etiquette wiring |
| **A** | `tests/elicitation_coverage_summary.rs` | Summary + shadow-core-support digest |
| **C** | `tests/coverage_parity.rs` | Framework std on homecoming/amenable |
| **Unit** | `tests/build_stage.rs`, `build::shadow_dep` tests | Build/cache keys |

**Workflow per extraction:**

1. Identify parity tests for the straight-port site (table above).
2. Implement hook path behind the same artifact paths.
3. Assert normalized equivalence (existing parity helpers).
4. Delete or demote straight-port code to `#[cfg(test)]` reference only.

---

## Rules for new work (post-R0)

1. **No new `collect/` modules.** Fact-gathering → enricher; judgment → assessor.
2. **No `parse_rustdoc_json` outside** `rustdoc_loader`, `build/`, and test fixtures.
3. **Reporters render findings only** — if you need a new input, add a probe or enricher.
4. **Cross-crate logic → WorkspaceAssessor** (or explicit workspace scope), not per-crate assessor calling collect.
5. **Gap taxonomy changes** go through `Coverage::classify_gap` / shared classifiers, not ad hoc branches in reporters.

---

## Open decisions

1. **WorkspaceAssessor seam** — new hook trait vs extended `Assessor` with workspace context?
2. **Workspace-scoped IR** — single merged graph vs per-crate IR with cross-crate query API?
3. **Enricher-declared loader needs** — associated type vs string id on `IrEnricher`?
4. **AttrStore schema** — string keys + JSON for enricher attrs vs typed plugin registry?
5. **Reference oracle retention** — how long to keep inventory-diff libraries for shadow/impl equivalence tests?

---

## References

- [CORDIAL_PLAN.md](../../CORDIAL_PLAN.md) — hook seams, IR, elicit_doc mapping table
- [coverage-as-plugin.md](coverage-as-plugin.md) — Plugin + Coverage phases C0–C7
- [elicit-doc-parity.md](elicit-doc-parity.md) — behavioral contract + baseline layout
- [elicit_doc pipeline layout](../../../elicit_doc/src/pipeline/layout.rs) — cache key origin
- [elicit_doc shadow stages](../../../elicit_doc/src/pipeline/stages/shadow_stages.rs) — port source for Pattern 2/3

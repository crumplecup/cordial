# Coverage as plugin

Planning document for cordial's **coverage plugin model**: how trait-impl
coverage, shadow diffs, and framework-std reports (elicitation, homecoming,
amenable) share one architecture instead of three parallel pipelines.

Architecture hooks and IR design live in [CORDIAL_PLAN.md](../../CORDIAL_PLAN.md).
Output parity targets live in
[elicit-doc-parity.md](elicit-doc-parity.md).

---

## Problem

In `elicit_doc`, "coverage" is one family of questions asked many times with
different orchestration:

| Profile | Our trait(s) | Target library | elicit_doc path |
| --- | --- | --- | --- |
| **Elicitation** | `ElicitComplete` + 8 supertraits | Workspace deps, shadow twins | impl + trenchcoat + shadow stages |
| **Homecoming** | `Code` | Rust `std` inventory | `framework_stages` (HomecomingStd) |
| **Amenable** | Registry / `RustStdStandard<…>` | Rust `std` + registry dump | `framework_stages` (AmenableStd) |

Each path reimplements: build rustdoc → inventory → "does type X satisfy our
trait requirement?" → gap taxonomy → CSV / checklist / summary.

Cordial's current scaffold (`impl_coverage`, `trenchcoat`, `shadow` etiquettes)
ports elicitation piecemeal as **three hook bundles** without a shared semantic
layer. Continuing P4–P5 by copying elicit_doc branches would recreate the
same duplication under different file names.

---

## Design goal

**Plugin** is the root registration trait. **Coverage** is one natural
*semantic supertrait* of Plugin — not a separate pipeline mode.

Other reusable semantics (`EvidenceSource`, …) appear as **sibling**
supertraits when we discover them. Concrete products (`ElicitationCoverage`,
`HomecomingStdCoverage`, user-defined profiles) implement the supertraits they
need and register as plugins.

Quality scanners (tracing, antipatterns, …) implement **Plugin only**.
Panicking APIs live on **ErrorHandling** (`StandardErrorHandling`), not as a
standalone quality plugin.
They share hooks and session machinery but do not implement `Coverage`.

The error-handling family implements **`ErrorHandling: Plugin`** as a sibling
supertrait — one registration surface for any workspace. See
[error-handling-as-plugin.md](error-handling-as-plugin.md).

---

## Layer model

```text
Session
  └── registers Plugin(s)
        │
        ├── Quality plugins (Plugin only)
        │     tracing, antipatterns, …
        │
        ├── ErrorHandling plugins (ErrorHandling: Plugin)
        │     StandardErrorHandling  (any workspace; includes panics)
        │
        └── Coverage plugins (Coverage: Plugin)
              ElicitationCoverage
              HomecomingStdCoverage
              AmenableStdCoverage
              (user crates)

Plugin ──► one or more Etiquette hook bundles
Etiquette ──► Loader / IrEnricher / Probe / Assessor / Reporter
```

| Layer | Responsibility | Stable? |
| --- | --- | --- |
| **Hooks** | IR mechanics: parse, enrich, mark, assess, render | Yes — shared infrastructure |
| **Etiquette** | Named hook bundle for one runnable unit | Yes — composition unit |
| **Plugin** | Identity + what to register with the session | Yes — registration seam |
| **Coverage** | Target selection, trait requirements, gap classification | Yes — coverage vocabulary |
| **ErrorHandling** | Workspace scope, analysis layers, error-flow policy | Yes — error-handling vocabulary |
| **Concrete profile** | Elicitation / homecoming / amenable / custom | Extensible — downstream crates |

Naming note: today the codebase uses **Etiquette** for the hook bundle.
This plan introduces **Plugin** as the registration trait. Etiquette remains
the mechanical layer; Plugin is what users register. We may later alias
`Etiquette` as "hook bundle" in docs only — no rename required for phase 1.

---

## The coverage question (shared semantics)

Every `Coverage` plugin answers the same logical query:

1. **Trait requirement** — what does "covered" mean? (single trait, composite
   like `ElicitComplete`, registry-backed witness, …)
2. **Target scope** — which types are in the denominator? (std inventory,
   workspace dep, shadow upstream, local crate, …)
3. **Gap judgment** — for each target type, covered or which gap kind?
4. **Artifacts** — CSV, checklist, gaps file, summary section

Shadow and trenchcoat are **not separate plugin kinds**. They are how
`ElicitationCoverage` chooses targets and IR edges (`Mirrors`, `Wraps`) inside
its `Coverage` implementation.

---

## Trait sketches (conceptual)

Not final API — guides implementation phases.

### `Plugin`

```rust
/// Runnable unit registered with the session.
pub trait Plugin: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;

    /// Hook bundles this plugin contributes (deduped across plugins in one run).
    fn etiquettes(&self) -> &[&dyn Etiquette];
}
```

Quality plugins return one etiquette. Coverage plugins may return several
(shared rustdoc loader + profile-specific assessors/reporters).

### `Coverage`

```rust
/// Semantic supertrait: trait-impl coverage over a target library.
pub trait Coverage: Plugin {
    /// Crates / type universes to analyze (replaces hardcoded TRACKED_TARGETS).
    fn targets(&self, session: &dyn SessionView) -> CordialResult<Vec<CoverageTarget>>;

    /// What impls count as "covered" for this profile.
    fn trait_requirement(&self) -> &dyn TraitRequirement;

    /// Map assessed state → gap kind (profile-specific taxonomy extension).
    fn classify_gap(&self, ctx: &GapContext) -> Option<ImplGapKind>;
}
```

Default `Plugin` impl for `T: Coverage` wires standard build + rustdoc hooks
and profile-specific assessors.

### Supporting traits (shared, not supertraits of Plugin)

| Trait | Role |
| --- | --- |
| **`TraitRequirement`** | Predicate over IR: prereqs, composite trait, single trait name |
| **`CoverageTarget`** | One built inventory scope: member crate, dep, std source, shadow pair |
| **`GapContext`** | Type node + prereqs + wrapper/shadow context for classification |
| **`TargetProvider`** | Config / roster source for `Coverage::targets` (file, const, cargo metadata) |

### Sibling supertraits (discover when needed)

When a semantic repeats across plugins but is **not** part of coverage, define
another supertrait of `Plugin` and implement it on concrete types:

```text
Plugin
  ├── Coverage          ← trait × target lib
  ├── EvidenceSource    ← (future) registry / witness linkage for amenable
  └── …                 ← discovered later; never optional fields on Coverage
```

`AmenableStdCoverage` might implement `Coverage + EvidenceSource`.
`ElicitationCoverage` implements `Coverage` only.
`PanicsPlugin` implements `Plugin` only.

Do **not** add `Option<EvidenceSource>` to `Coverage`. Compose traits on the
concrete type instead.

---

## Concrete profiles

### `ElicitationCoverage`

| Aspect | Choice |
| --- | --- |
| Trait requirement | `ElicitComplete` + 8 supertraits (`TraitPrereqs`) |
| Targets | Workspace members + tracked upstream deps + shadow pairs |
| Gap kinds | `MissingOurTraits`, `ReadyForElicitComplete`, `ExternallyBlocked`, `FeatureGatedExternal` |
| Extra edges | `Implements`, `Wraps`, `Mirrors` |
| Hooks | Build, rustdoc loader, trait impl enricher, trenchcoat enricher, shadow link enricher, impl gap assessor |
| Artifacts | `impl-coverage.csv`, `gaps-impl.csv`, trenchcoat + shadow reports, coverage section in `summary.md` |
| Crate | `cordial_elicitation` (re-export); hooks may stay in `cordial` behind features |

### `HomecomingStdCoverage`

| Aspect | Choice |
| --- | --- |
| Trait requirement | Single trait: `Code` |
| Targets | Sysroot std inventory (core/alloc/std merge) |
| Gap kinds | Complete / Missing / Skipped (patch-documented) |
| Targets provider | Framework impl crate: `homecoming_core` |
| Artifacts | Framework CSV, checklist, gaps CSV, summary |
| Crate | `cordial_elicitation` or `cordial_homecoming` |

### `AmenableStdCoverage`

| Aspect | Choice |
| --- | --- |
| Trait requirement | Registry-backed std surface (`RustStdStandard<…>`) |
| Targets | Sysroot std + amenable registry dump |
| Extra supertrait | Likely `EvidenceSource` when registry semantics stabilize |
| Gap kinds | Amenable-specific status + compliance rows |
| Artifacts | amenable std CSV, checklist, gaps, summary |
| Crate | `cordial_elicitation` or `cordial_amenable` |

### User-defined coverage

A third-party crate implements `Coverage` + `Plugin` on a **named product
type** and registers it with `session.register_plugin(...)`. Worked
templates for all three plugin kinds (quality `StaticPlugin`, `Coverage`,
`ErrorHandling`) live in [`examples/custom_plugins`](../../examples/custom_plugins);
see [custom-plugin-example.md](custom-plugin-example.md).

---

## Shared infrastructure (already started)

These stay in `cordial` core — **not** duplicated per profile:

| Component | Status | Path / notes |
| --- | --- | --- |
| `cordial build rustdoc` | Done | `src/cargo_rustdoc/` |
| Rustdoc loader + inventory | Done | `src/rustdoc_loader.rs`, `src/rustdoc/` |
| `TraitPrereqs` + gap kinds | Done | `src/rustdoc/prereqs.rs`, impl_coverage assessor |
| `Implements` / `Wraps` / `Mirrors` edges | Partial | enrichers in `src/enricher/` |
| Store cache layout | Done | `cache/rustdoc/`, `cache/builds/` |
| Exception / skip patches | Done | `src/exceptions.rs` |

Profiles **compose** this infrastructure; they do not fork it.

---

## Migration from current etiquettes

Today (pre-refactor):

```
impl_coverage etiquette  ─┐
trenchcoat etiquette     ─┼─► cordial coverage CLI / elicitation feature
shadow etiquette         ─┘
```

Target (post-refactor):

```
ElicitationCoverage: Coverage + Plugin
  └── etiquettes: [shared rustdoc hooks, impl gap, trenchcoat, shadow reporters]
```

### Step-by-step

| Phase | Work | Exit |
| --- | --- | --- |
| **C0 — Document** | This plan; cross-link parity doc | Approved model |
| **C1 — Plugin seam** | Introduce `Plugin` trait; `session.register_plugin()`; quality etiquettes adapt via thin `Plugin` wrappers | `cordial run` unchanged |
| **C2 — Coverage trait** | Define `Coverage`, `TraitRequirement`, `CoverageTarget`, `TargetProvider`; default gap classification using existing `TraitPrereqs` | Unit tests on trait contracts |
| **C3 — ElicitationCoverage** | Move impl/trenchcoat/shadow wiring into one `ElicitationCoverage` type; deprecate registering three etiquettes separately | Parity tests on `minimal-workspace` |
| **C4 — Target roster** | Port `TRACKED_TARGETS` → `ElicitationTargetProvider`; shadow pair discovery | Shadow no longer needs hand-edited `shadow-map.json` for roster |
| **C5 — HomecomingStdCoverage** | Port framework std pipeline as `Coverage` impl | homecoming workspace parity |
| **C6 — AmenableStdCoverage** | Port amenable std + registry; introduce `EvidenceSource` if semantics warrant | amenable workspace parity |
| **C7 — Summary + CLI** | `coverage summary.md` rolls up registered coverage plugins; `cordial coverage` runs all registered coverage plugins | P4/P5 parity exit |

Phases C1–C3 can proceed **before** finishing every P4 parity line item; they
reduce rework for shadow, feature probes, and summary.

### Priority (2026-03)

**Defer elicitation coverage parity** until the generalized model is proven on
framework profiles. Implement **C5 → C6** next, with parity tests against
`elicit_doc` on the **homecoming** and **amenable** workspaces (Tier C).

Rationale:

- Homecoming (`Code`) and amenable (registry-backed `RustStdStandard<T>`) exercise
  the same `Coverage` seam with a **simpler denominator** (merged std inventory)
  and **clearer gap taxonomy** than elicitation's shadow/upstream/trenchcoat stack.
- Passing parity there validates trait requirements, target providers, assessors,
  and reporters as reusable infrastructure — the generalization win.
- Elicitation-specific gaps (shadow mirror compare, impl-dep builds, trenchcoat JSON)
  remain on the backlog; they compose the same hooks once framework std is solid.

**Parity harness order:** port `elicit_doc/tests/framework_std_test.rs` and
`amenable_registry_test.rs` semantics first (unit), then Tier C workspace diff
(`homecoming/`, `amenable/` repos) for CSV + summary metrics.

**Post-parity (implementation shape):** once behavioral parity is frozen, structural
convergence onto hook seams is tracked in
[post-parity-alignment.md](post-parity-alignment.md) (strangler map, extraction
patterns, phases R0–R7).

---

## Session and CLI

### Registration

```rust
let mut session = SessionBuilder::new(root)
    .register_plugin(&ElicitationCoverage::DEFAULT)
    .register_plugin(&PanicsPlugin::DEFAULT)  // Plugin only
    .build();
```

Filter by plugin id or category (`quality` vs `coverage`).

### CLI (evolution)

| Command | Behavior |
| --- | --- |
| `cordial run` | All registered plugins (quality + coverage) |
| `cordial quality` | Plugins where `Coverage` is **not** implemented |
| `cordial coverage` | Plugins implementing `Coverage` |
| `cordial build rustdoc` | Shared build stage for coverage targets |

---

## Parity mapping

Updates [elicit-doc-parity.md](elicit-doc-parity.md) phases:

| Parity phase | Coverage plugin work |
| --- | --- |
| **P4** (elicitation) | C2–C4: ElicitationCoverage, targets, shadow, summary |
| **P5** (framework) | C5–C6: HomecomingStdCoverage, AmenableStdCoverage |

Parity tests remain **artifact-based**. Plugin refactor must not change
normalization keys without updating baselines.

---

## Crate layout

```
cordial/                      # Plugin, Coverage traits; shared hooks; build; IR
cordial_elicitation/          # ElicitationCoverage, re-exports
cordial_homecoming/           # (future) HomecomingStdCoverage
cordial_amenable/             # (future) AmenableStdCoverage
cordial_cli/                  # registers default plugin set via features
user crate                    # impl Coverage + Plugin
```

Dependency direction unchanged: domain crates depend on `cordial`, not reverse.

---

## Open questions

1. **One etiquette vs many per coverage plugin** — `ElicitationCoverage` likely
   exposes 3–4 etiquettes internally for assessor isolation; session dedupes
   loaders. Confirm this stays an implementation detail behind `Plugin`.

2. **Workspace-level assessment** — shadow diff and coverage summary may need
   `WorkspaceAssessor` hook (cross-crate pass after per-crate assess). Add when
   C4 lands, not on `Coverage` trait initially.

3. **Plugin vs Etiquette rename** — keep both names long-term (Etiquette =
   hooks, Plugin = product) or collapse later?

4. **Feature gates** — `cordial/elicitation` enables `ElicitationCoverage`;
   framework profiles get separate features. `Plugin` registry respects features.

5. **Static vs dynamic registration** — v1 stays `&'static dyn Plugin` like
   etiquettes today; dynamic loading out of scope.

---

## References

- [CORDIAL_PLAN.md](../../CORDIAL_PLAN.md) — Etiquette, hooks, IR, `TargetProvider` note
- [elicit-doc-parity.md](elicit-doc-parity.md) — P4/P5 coverage parity
- `elicit_doc/src/pipeline/config.rs` — `TRACKED_TARGETS`, `CoverageProfile`
- `elicit_doc/src/pipeline/stages/framework_stages.rs` — homecoming / amenable std
- `elicit_doc/src/gaps/impl.rs` — gap taxonomy reference
- `elicit_doc/src/framework_std/` — framework trait coverage reports

---

## Status

| Phase | Status |
| --- | --- |
| C0 — Document | **Active** (this doc) |
| C1 — Plugin seam | **Complete** — `Plugin`, `EtiquettePlugin`, `register_plugin()`, CLI via plugin registry |
| C2 — Coverage trait | **Complete** — `Coverage`, `TraitRequirement`, `CoverageTarget`, `TargetProvider`, gap classification |
| C3 — ElicitationCoverage | **Complete** — bundles impl-coverage + trenchcoat + shadow; `cordial coverage` uses plugin id `elicitation-coverage` |
| C4 — Target roster | **Complete** — `ELICITATION_TRACKED_TARGETS`, `ElicitationTargetProvider`, roster shadow pair discovery |
| C5 — HomecomingStdCoverage | **Complete** — `framework_std` module, `HomecomingStdCoverage` plugin, workspace hub detection, `HOMECOMING_STD_ETIQUETTE` reporter; sysroot cache at `~/.cordial/sysroot`; `cordial build sysroot` |
| C6 — AmenableStdCoverage | **Complete** — registry dump, witness layers, proof harness scan, `AmenableStdCoverage` plugin, hub routing |
| C7 — Summary + CLI | **Complete** — session-level coverage rollup (`summary.md` / `coverage-summary.md`); quality summary gated to quality runs |

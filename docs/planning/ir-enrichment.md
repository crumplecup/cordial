# IR enrichment — one-stop shop

Planning document for making the **graph IR the single authoritative store**
for analysis facts. Replaces the post-parity compromise of a thin petgraph plus
`RustdocInventory` side caches.

Complements [CORDIAL_PLAN.md](../../CORDIAL_PLAN.md) (IR design) and
[post-parity-alignment.md](post-parity-alignment.md) (hook migration, complete).

---

## Problem

Today cordial runs **two parallel representations** of rustdoc coverage data:

```text
RustdocLoader::populate_ir     →  type/trait nodes (qualified_path, kind)
RustdocStructureEnricher       →  attrs (methods, impls, prereqs, trenchcoat, …)
WorkspaceIr.wrapper_coverage_map → hub wrapper coverage (impl_coverage)
```

Enrichers and assessors read the graph:

| Consumer | Reads graph | Reads inventory / re-parse |
| --- | --- | --- |
| Impl gap assessor | type nodes, enricher attrs | — |
| TraitImplEnricher | `trait_impls` attrs + edges | — |
| TrenchcoatEnricher | `wraps_foreign` attrs + edges | — |
| Cross-crate shadow | graph attrs + cross-crate edges | — (oracle in `cordial::testing`) |
| WrapperCoverageEnricher | workspace hub map + attrs | — |
| Feature probe build | probe cache only | build-time rustdoc for probes |
| Digest type counts | graph node count | — |
| Framework std (sysroot) | Implements edges (hub) | sysroot inventory in probe |

**Target:** every row in the right column moves to the graph (or to a
workspace-level graph query). `parse_rustdoc_json` runs only inside
`RustdocLoader` (and `build/` for cache generation). Nothing downstream holds
`RustdocInventory`.

---

## Definition of done

**IR is a one-stop shop when:**

1. **No hot-path `RustdocInventory`** — session, probes, assessors, reporters,
   and digests never call `parse_rustdoc_json` or hold session-side inventory caches.
   `parse_rustdoc_json` is confined to the loader, on-demand crate preload, probe
   cache build, and `cordial::testing` oracles (`tests/architecture.rs` enforces).
2. **Coverage questions are graph queries** — impl prereqs, shadow mirror rows,
   method checklists, wrapper coverage, and trait impl presence are answered
   from nodes, edges, and attrs via `IrView` / `Query`.
3. **Cross-crate logic uses workspace IR** — shadow pairing and hub wrapper
   maps are materialized as cross-crate edges or workspace attrs, not inventory
   diff libraries.
4. **Parity preserved** — Tier A tests stay green; inventory diff remains only
   in `cordial::testing` oracles until IR path is proven equivalent, then
   deleted.

---

## Target architecture

```text
                    ┌─────────────────────────────────────┐
  RustdocLoader     │  parse JSON once per cache key       │
        │           └─────────────────────────────────────┘
        ▼
  populate_ir (skeleton) ──► CrateIr: Item nodes + Contains edges
        │
        ▼
  RustdocStructureEnricher (new, priority 1) ──► attrs + Implements/Defines/…
        │
        ▼
  Domain enrichers (feature probe, wrapper, shadow link, …) ──► plugin attrs
        │
        ▼
  Probe → Assessor → Reporter     (read IrView only)
        │
        ▼
  WorkspaceAssessor (cross-crate)  (read WorkspaceIr + cross_crate_edges)
```

**Key decision:** rustdoc JSON is **fully materialized in one enricher pass**
immediately after loader skeleton, while `RustdocLoadView` is still available.
Downstream enrichers stop downcasting to `RustdocLoadView` for fact extraction;
they only add plugin-specific attrs (feature probes, proof harness, etc.).

---

## Consumer → IR fact map

Facts already on the graph (keep):

| Fact | Representation |
| --- | --- |
| Crate version | root attr `crate_version` |
| Public type/trait identity | Item node + `qualified_path` |
| `impl Trait for Type` | `Implements` edge (TraitImplEnricher) |
| Trenchcoat wrapper | `Wraps` edge |
| Same-crate shadow pair | `Mirrors` edge + `shadow_path` attr |
| Feature probe result | attrs on type node |
| Proof harness status | attrs on type node |
| Wrapper coverage (hub) | attr on type node |
| Error-site partition | attrs + `ErrorFlow` edges |

Facts **missing** from the graph (must add):

| Fact | Needed by | Proposed IR |
| --- | --- | --- |
| Item `name`, `is_public` | shadow matching, inventory row kind | item attrs |
| Public methods per type | shadow method checklist | `Defines` edges (Type → Fn item nodes) **or** attr `public_methods: ["draw", …]` |
| Trait impl map (short name → impl) | shadow trait checklist | attr `trait_impls: ["Serialize", …]` on type node |
| ElicitComplete prereqs per type | impl gap assessor | attr `trait_prereqs` (JSON `TraitPrereqs`) on type node |
| ElicitComplete set membership | shadow verification | attr `elicit_complete: bool` on type node |
| Stability / nightly gating | framework std scope | attrs `is_unstable`, `stability_level` on item node |
| Type alias target | amenable registry | attr `alias_target` on item node |
| Generic parameter presence | framework std rows | attr `is_generic` on item node |
| Cross-crate shadow pairing | cross-crate shadow | workspace `Mirrors` cross-crate edges (upstream crate node ↔ shadow crate node) |
| Hub wrapper map | wrapper enricher | workspace attr or `Wraps` cross-crate from hub types |

---

## Attr schema (v1)

Centralize keys in `src/ir/attrs.rs` (new module). Values are JSON until typed
AttrStore lands (open decision from post-parity).

### Crate root

| Key | Type | Source |
| --- | --- | --- |
| `crate_version` | string | loader (exists) |
| `ir_origin` | string | loader (exists) |

### Item (type / trait)

| Key | Type | Source |
| --- | --- | --- |
| `qualified_path` | string | loader (exists) |
| `rustdoc_kind` | string | loader (exists) |
| `item_name` | string | loader |
| `is_public` | bool | loader |
| `is_generic` | bool | rustdoc structure enricher |
| `is_unstable` | bool | rustdoc structure enricher |
| `alias_target` | string? | rustdoc structure enricher |
| `public_methods` | string[] | rustdoc structure enricher |
| `trait_impls` | string[] | rustdoc structure enricher (trait short names) |
| `trait_prereqs` | object | rustdoc structure enricher (`TraitPrereqs`) |
| `elicit_complete` | bool | rustdoc structure enricher |

### Plugin enrichers (unchanged keys, document in attrs.rs)

| Key | Enricher |
| --- | --- |
| `feature_probe_*` | FeatureProbeEnricher |
| `wrapper_coverage` | WrapperCoverageEnricher |
| `proof_test`, `composition_test` | ProofHarnessEnricher |
| `shadow_path` | ShadowLinkEnricher |

---

## Edge usage (extend)

| Edge | Use for enrichment |
| --- | --- |
| `Implements` | type → trait (exists) |
| `Wraps` | wrapper → foreign (exists) |
| `Mirrors` | shadow item → upstream item; **cross-crate** via `WorkspaceIr.cross_crate_edges` |
| `Defines` | optional: type → method item nodes (if we want method-level probes) |
| `Depends` | future: CargoLoader crate → crate deps |

**Recommendation:** start with **attrs** for method/trait impl sets (simpler,
matches current shadow diff shape). Add `Defines` method nodes only if probes
need per-method anchors.

---

## Query helpers (new, `src/ir/rustdoc_query.rs`)

Thin read API so probes/assessors do not parse JSON attrs ad hoc:

```rust
fn type_public_methods(ir: &dyn IrView, type_path: &str) -> &[str];
fn type_trait_impls(ir: &dyn IrView, type_path: &str) -> &[str];
fn type_trait_prereqs(ir: &dyn IrView, type_path: &str) -> Option<TraitPrereqs>;
fn type_elicit_complete(ir: &dyn IrView, type_path: &str) -> bool;
fn mirror_target(ir: &dyn IrView, type_node: NodeId) -> Option<NodeId>; // via Mirrors
```

Cross-crate variants take `WorkspaceIr` + crate names.

These replace direct attr string lookups in assessors and enable shadow diff
rewritten as IR comparison.

---

## Phases

### I1 — Rustdoc structure enricher ✅

| Task | Detail | Status |
| --- | --- | --- |
| Add `src/ir/attrs.rs` | Canonical attr key constants | done |
| Add `RustdocStructureEnricher` | priority 1, `required_loader = rustdoc` | done |
| Materialize | `item_name`, `is_public`, `is_generic`, `alias_target`, stability | done |
| Materialize | `public_methods`, `trait_impls`, `trait_prereqs`, `elicit_complete` | done |
| Wire | auto-inject in session when rustdoc loader present (like PathIndexEnricher) | done |
| Expand `populate_ir` | Set `item_name` / `is_public` at loader if cheaper | done |
| Query helpers | `src/ir/rustdoc_query.rs` | done |
| Unit tests | `tests/rustdoc_structure_enricher.rs` | done |

**Exit:** `TraitImplEnricher`, `TrenchcoatEnricher` can use graph-only paths
(inventory optional for transition). Unit tests on attrs for fixture crates.

**Unlocks:** impl assessor stops depending on enricher order for prereqs;
trait impl data always present before FeatureProbeEnricher.

---

### I2 — Impl / trenchcoat on graph only ✅

| Task | Detail | Status |
| --- | --- | --- |
| Refactor `TraitImplEnricher` | Read `trait_impls` attr **or** walk `Implements` edges only | done |
| Refactor `TrenchcoatEnricher` | Use `wraps_foreign` attr from structure enricher | done |
| Refactor impl probe/assessor | Use `ir::rustdoc_query` helpers | done |
| Remove | downcast to `RustdocLoadView` in impl/trenchcoat/feature-probe enrichers | done |
| Unit tests | `tests/impl_graph_enrichers.rs` | done |

**Exit:** no `LoadView` inventory downcast in impl/trenchcoat etiquettes.

**Unlocks:** FeatureProbeEnricher reads prereqs from IR, not re-collect from JSON.

---

### I3 — Shadow on graph ✅

| Task | Detail | Status |
| --- | --- | --- |
| Add workspace pass | `materialize_cross_crate_shadow_mirrors` inserts `cross_crate_edges` Mirrors | done |
| Add `build_shadow_pair_report_from_workspace_ir` | compare attrs across paired nodes | done |
| Switch | `CrossCrateShadowWorkspaceAssessor` via `build_shadow_pair_report_from_workspace` | done |
| Retire hot path | `preload_shadow_pair_inventories` + session inventory cache | done |
| Preload | `preload_shadow_pair_crates` loads missing pair IR + enrichers | done |
| Digest | type counts from graph IR | done |
| Unit tests | `tests/shadow_ir_compare.rs` | done |

**Exit:** shadow parity tests green with IR path; inventory diff oracle only in
`cordial::testing`.

**Unlocks:** digest type counts from graph node count; shadow summary from findings only.

---

### I4 — Wrapper / hub on workspace IR

| Task | Detail |
| --- | --- |
| Materialize hub wrapper map | during workspace enrich: `Wraps` or attrs from elicitation hub crate IR (already loaded) | done |
| Refactor `WrapperCoverageEnricher` | read workspace hub IR, not `load_workspace_wrapper_coverage()` | done |
| Cache | hub map as workspace-level attr keyed by foreign type path | done |
| Shared crate load | `src/ir/crate_load.rs` for hub + shadow preload | done |
| Unit tests | `tests/wrapper_ir.rs` | done |

**Exit:** no `parse_rustdoc_json` in `workspace_wrapper.rs` on hot path.

**Unlocks:** single load per hub crate; wrapper coverage in sync with impl enrichers.

---

### I5 — Cleanup and enforcement

| Task | Detail |
| --- | --- |
| Remove | `WorkspaceIr.rustdoc_inventories` | done |
| Shrink | `RustdocLoadView` to loader-only (inventory `pub(crate)`) | done |
| Add | `tests/architecture.rs` allowlist guard | done |
| Move | inventory diff oracles to `src/testing/{shadow,wrapper}_oracle.rs` | done |
| Update | CORDIAL_PLAN IR section + attr schema | done |

**Exit:** post-parity open decision #2 (workspace IR) resolved: per-crate graphs +
`cross_crate_edges` + workspace attrs; no merged single graph required.

---

## What becomes obvious after each phase

| After | Next moves |
| --- | --- |
| **I1** | Typed attr registry; consolidate enricher priorities; framework std probe reads item attrs not full inventory scan |
| **I2** | Feature probe uses IR prereqs; delete duplicate `collect_trait_prereqs` in assessor path |
| **I3** | ShadowMethodChecklist as assessor findings from IR diff; delete `shadow/report.rs` hot-path exports |
| **I4** | Digest fully findings + IR; delete wrapper side loader |
| **I5** | Plugin author docs with stable query helpers; `cordial-plugin` crate exposing `IrView` extensions only |

---

## Open decisions (resolved for this plan)

| Question | Decision |
| --- | --- |
| Merged workspace graph vs per-crate + cross edges? | **Per-crate + `cross_crate_edges`** — matches current `WorkspaceIr`, lower migration cost |
| Methods as nodes vs attrs? | **Attrs first** (`public_methods`, `trait_impls`); method nodes only if needed |
| Keep `krate` JSON on root? | **No** — materialize once in I1, drop embedded `Crate` from session state |
| AttrStore typing? | **String keys + JSON v1**; document in `ir/attrs.rs`; typed wrappers in query helpers |
| Workspace enricher seam? | **Add `WorkspaceEnricher`** hook in I3 if cross-crate materialization does not fit assessor prep |

---

## Parity guard

| Tier | Tests | Phase gate |
| --- | --- | --- |
| A | `shadow_pair_gaps`, `feature_probe_gaps`, `wrapper_coverage_gaps`, `elicitation_parity` | I2–I3 |
| A | `elicitation_coverage_summary` | I3–I4 |
| Unit | `rustdoc_structure_enricher` fixture tests (new) | I1 |

Workflow: IR path behind same artifacts → assert equivalence with inventory
oracle → delete oracle hot-path call.

---

## Suggested order

```text
I1 structure enricher + attrs.rs + query helpers
  → I2 impl/trenchcoat graph-only
  → I3 shadow IR compare + drop inventory cache
  → I4 hub wrapper on workspace IR
  → I5 cleanup + enforcement
```

I1 is the **critical path**; everything else fans out from having complete
rustdoc facts on type nodes.

---

## References

- [CORDIAL_PLAN.md](../../CORDIAL_PLAN.md) — edge taxonomy, IrView/IrMut, query layer
- [post-parity-alignment.md](post-parity-alignment.md) — Pattern 1/2 enricher templates
- `src/rustdoc/{method_maps,impls,prereqs,elicit_complete}.rs` — extraction logic to fold into I1
- `src/rustdoc_loader.rs` — current thin `populate_ir`

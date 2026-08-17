# elicit_doc feature parity

Planning document for reaching **output parity** with [`elicit_doc`](../../../elicit_doc):
every analysis `elicit_doc` produces today should have a cordial etiquette (or
deliberate superset) whose artifacts are **as good or better** when compared
against the same workspace.

Architecture and plugin seams are covered in [CORDIAL_PLAN.md](../../CORDIAL_PLAN.md).
Coverage plugin design (elicitation / homecoming / amenable profiles) is in
[coverage-as-plugin.md](coverage-as-plugin.md).
This document covers **what to port**, **how to verify it**, and **in what order**.

Complements [elicit_doc PLANNING_INDEX](../../../elicit_doc/PLANNING_INDEX.md).

---

## Goal

`cordial` replaces `elicit_doc` as the analysis engine. Parity is measured on
**observable outputs** — CSV rows, checklist items, gap classifications,
summary metrics — not on internal pipeline shape.

Success means:

1. Running cordial on a reference workspace produces artifacts that pass parity
   tests against a frozen `elicit_doc` baseline for every ported scanner.
2. Cordial may **strictly improve** on elicit_doc (extra columns, finer rules,
   better context) as long as tests document the improvement and baseline
   expectations are updated intentionally.
3. `elicit_doc` can delegate to `cordial_elicitation` / `cordial_cli` without
   losing report fidelity.

---

## Parity standard: “as good or better”

For each artifact pair `(elicit_doc, cordial)` on the **same project root**
with the **same exception patches** applied:

| Dimension | Pass condition |
| --- | --- |
| **Open-finding recall** | Every *open* row in elicit_doc appears in cordial (same rule id, same anchor file:line ± tolerance). Suppressed/exception rows may differ in representation but open totals must not shrink. |
| **Precision** | Cordial must not emit spurious *open* findings elicit_doc would not. New rules are allowed if gated behind etiquette features and documented. |
| **Summary metrics** | Workspace summary counts (open items, gap kinds, coverage %) match elicit_doc or explain documented differences (e.g. cordial detects `.unwrap` once both tools implement it). |
| **CSV schema** | Cordial columns are a **superset** of elicit_doc columns for the same report, or a stable column mapping is tested. Extra columns (`disposition`, `suppression_reason`) are encouraged. |
| **Checklist usability** | Checklist markdown lists every open item with equivalent context (module/fn, snippet). Ordering may differ; content must not omit actionable rows. |
| **Cache equivalence** | Where cordial replaces an elicit_doc cache JSON, normalized content must round-trip the same facts (see [Cache mapping](#cache-mapping)). |

**Normalization before compare**

- Resolve paths relative to crate root.
- Sort rows by stable key: `(rule_id, file, line, context)`.
- Strip generation timestamps and `_Generated:` footers.
- Map disposition: elicit_doc “open checklist row” ≡ cordial `Disposition::Open`.
- Apply the same exception patch files from `{store}/quality/patches/` or
  `{store}/patches/`.

**When cordial is strictly better**

Record in the parity test as an explicit `improvement` annotation and bump the
baseline after review. Examples: detecting `.unwrap`, scanning `tests/` (excluding
fixtures), richer error-chain join keys.

---

## Comparison workflow

```text
Reference workspace(s)
        │
        ├─► elicit_doc run  ──► ~/.elicit_doc/{slug}/   ──► freeze baseline
        │                              │
        │                              ▼
        │                    tests/parity/baseline/{slug}/...
        │
        └─► cordial run     ──► ~/.cordial/{slug}/       ──► parity test diff
                                       │
                                       ▼
                             assert: recall + precision + metrics
```

### Reference workspaces (tiered)

| Tier | Workspace | Purpose |
| --- | --- | --- |
| **A — CI fixtures** | `elicit_doc/tests/fixtures/minimal-workspace` | Coverage pipeline smoke |
| **A — CI fixtures** | `elicit_doc/tests/fixtures/quality/**` (per scanner) | Quality scanner unit parity |
| **B — Tool self-scan** | `elicit_doc` repo root | Full quality `run_quality_all` baseline |
| **C — Production** | `elicitation`, `amenable`, `homecoming` | Pre-release sign-off (manual/nightly) |

Tier A baselines are committed under `tests/parity/`. Tier B/C baselines
live in CI cache or are generated locally via `just parity-freeze`.

### Baseline freeze (one-time per workspace + elicit_doc version)

```bash
# From elicit_doc checkout
elicit_doc run --project /path/to/workspace          # coverage
elicit_doc quality all --project /path/to/workspace  # quality

# Copy normalized artifacts into cordial fixtures
rsync -a ~/.elicit_doc/{slug}/ \
  ../cordial/tests/parity/baseline/{slug}/
# Strip logs, manifest timestamps; keep CSV, md, json caches
```

Record in `tests/parity/MANIFEST.toml`:

- `elicit_doc` git rev
- workspace path
- command invocations
- feature flags / patches applied

### Parity test harness (to implement)

New integration test crate or `tests/parity/` module:

```rust
// Conceptual API
parity::compare_csv(
    baseline: "parity/baseline/minimal/panics.csv",
    actual: cordial_store.join("findings/panics.csv"),
    key_columns: &["kind", "file", "line", "context"],
    required_columns: &["crate", "kind", "context", "file", "line", "snippet"],
);
parity::compare_open_recall(...);
parity::compare_summary_metrics(...);
```

Harness lives in `tests/parity/mod.rs` (or `cordial_parity` dev-dependency crate).
Uses `similar` / custom diff for friendly failures. Fails on missing open rows;
warns on extra cordial-only columns.

Run in CI:

```bash
cargo test -p cordial --features full --test parity
```

---

## Store layout mapping

| elicit_doc (`~/.elicit_doc/{slug}/`) | cordial (`~/.cordial/{slug}/`) | Parity notes |
| --- | --- | --- |
| `coverage/` | `findings/` (coverage etiquettes) | Different top-level name; map by filename |
| `quality/` | `findings/` (quality etiquettes) | Cordial flattens; use etiquette prefix or subdirs later if needed |
| `cache/builds/`, `inventories/`, `extracts/`, `assessed/` | `cache/{crate}.ir.json`, `{crate}.ir.digests.json` | Cordial IR cache replaces many intermediate JSONs; parity tests compare **derived reports**, not 1:1 cache files |
| `cache/quality/{crate}.*.scan.json` | _(future)_ `{crate}.{etiquette}.scan.json` | Optional scan cache per etiquette |
| `patches/`, `quality/patches/` | `exceptions/` | JSON patch format already ported; verify path conventions |
| `snapshots/` | _(not yet)_ | Drift digests — defer or port as `cordial snapshot` |

---

## Artifact inventory

Status key: **done** = ported with parity tests · **partial** = simplified · **planned** · **n/a**

### Quality (`elicit_doc quality …`)

| Scanner | elicit_doc artifacts | cordial artifacts | Status |
| --- | --- | --- | --- |
| **Panics** | `panics.csv`, `panics.checklist.md`, `panics-summary.md`, `{crate}.panics.scan.json` | `findings/panics.csv`, `panics.checklist.md`, `panics-summary.md` | **done** — includes `.unwrap`; scans `src/` + `tests/`; CSV adds a `surface` column (library / binary / test) |
| **Tracing** | `tracing-instrument.csv`, `.checklist.md`, `-summary.md` | same names under `findings/` | **partial** — detect only; no `tracing apply` |
| **Derives** | `derives.csv`, `.checklist.md`, `-summary.md` | same under `findings/` | **done** |
| **Allows** | `allows.csv`, `.checklist.md`, `-summary.md` | same under `findings/` | **done** |
| **Modularity** | `modularity.csv`, `.checklist.md`, `-summary.md` | same under `findings/` | **done** |
| **Error sites** | `error-sites.csv`, partition CSV/summary | `error-sites.csv`, `.checklist.md`, `-summary.md`, `-partitioned.csv`, `-partition-summary.md` | **done** — inventory + partition |
| **Error chain** | `error-chain-preserved.csv`, `.checklist.md`, `-summary.md` | same under `findings/` | **done** — 5 preservation probes |
| **Internal error chain** | `internal-error-type-graph.csv`, `internal-error-compliance.csv`, checklist, summary | same under `findings/` | **partial** — type graph + compliance done |
| **Foreign error types** | `foreign-error-types.csv`, … | same under `findings/` | **done** |
| **Foreign error attenuation** | attenuation CSV, summary | same under `findings/` | **done** |
| **Antipatterns** | `antipatterns.csv`, `.checklist.md`, `version-in-member.csv`, summaries | same under `findings/` | **done** — Tier A fixtures + Tier C amenable dual-run (`tests/quality_parity.rs`) |
| **Unified quality** | `quality-report.md`, `summary.md` | `findings/quality-report.md`, `findings/summary.md` | **done** |

### Coverage (`elicit_doc run …`)

| Pipeline | elicit_doc artifacts | cordial artifacts | Status |
| --- | --- | --- | --- |
| **Impl coverage** | `{crate}.csv`, `{crate}.checklist.md`, `gaps-impl.csv`, `impl-gaps.json`, per-crate assessed JSON | `impl-coverage.csv`, checklist | **partial** — `Serialize`/`Deserialize`/`Debug` only; no `ElicitComplete`, gap taxonomy, harness |
| **Internal coverage** | `internal.csv` | — | planned |
| **Trenchcoat** | `trenchcoats.csv`, `trenchcoats.report.json`, wrapper-coverage JSON | trenchcoat CSV + `cache/wrapper-coverage.json` | **done** — wrapper map feeds impl gap classification |
| **Shadow** | per-target CSV/checklist, `gaps-shadow.csv`, `shadow-gaps.json` | `shadow-{upstream}.csv`, `gaps-shadow.csv`, `shadow-{upstream}.checklist.md`, same-crate `shadow.csv` | **partial** — cross-crate mirror + method/trait diff checklists + shadow-dep build/cache; Tier A baselines |
| **Framework std** | `std.csv`, `std.checklist.md`, `gaps-impl.csv`, amenable-specific reports | `std.csv`, checklist, gaps under `findings/` | **done** — Tier C dual-run green (homecoming + amenable `std.csv`, frozen gaps baselines) |
| **Executive summary** | `coverage/summary.md` | `findings/summary.md` (coverage-only) / `coverage-summary.md` (with quality) | **done** — impl + shadow metric tables + target support digest |
| **Build orchestration** | `cache/builds/*.build.json`, rustdoc copy | `cordial build rustdoc` + shadow-dep builds | **partial** — member + shadow-dep cache keys; sysroot via `cordial build sysroot` |
| **Proof harness** | `proof-harness.json`, test status in impl rows | `proof_test`, `composition_test` on `impl-coverage.csv` | **done** — scans hub proof test files via `collect::ProofHarness` |
| **Tracked targets** | `TRACKED_TARGETS` roster, shadow-dep cache keys | `ELICITATION_TRACKED_TARGETS`, `ElicitationTargetProvider`, `shadow-dep-{shadow}-{upstream}` | **done** — roster + cache stem parity with elicit_doc |

---

## Implementation phases

Each phase ends with **parity tests green** on Tier A fixtures before moving on.

### Phase P0 — Parity harness (foundation)

- [x] `tests/parity/` layout + `MANIFEST.toml` convention
- [x] `tests/parity_support/mod.rs`: CSV normalizers, open-finding recall assert
- [x] Freeze Tier A baselines from `elicit_doc` for panics + tracing on quality fixtures
- [x] CI job: `cargo test --features quality --test parity` on Tier A only

**Exit:** panics + tracing parity tests pass on at least two elicit_doc quality fixtures.

### Phase P1 — Quality scanner parity (batch 1)

- [x] Panics: add `.unwrap`, `tests/` scan (via `quality_scan_trees`), parity vs baseline
- [x] Tracing: parity on `mixed_visibilities`, `simple_fn` fixtures
- [x] Derives etiquette (5 rules)
- [x] Allows etiquette
- [x] Modularity etiquette
- [x] Parity tests for panics, tracing, allows, derives

**Exit (partial):** Tier A parity green on fixture workspaces; Tier B (`elicit_doc` self-scan) deferred.

### Phase P2 — Quality scanner parity (batch 2 — error handling)

Largest lint gap; multi-probe etiquettes with assessor dependencies:

- [x] Error sites inventory + partition (`error_sites` etiquette, parity on `error_sites` workspace)
- [x] Error chain preservation (5 probes — `error_chain` etiquette, parity on `error_chain` workspace)
- [x] Internal error chain + compliance graph (`internal_error_chain` etiquette, parity on compliance CSV)
- [x] Foreign error types + attenuation
- [x] Unified `quality-report.md` matching elicit_doc section order (+ `summary.md` workspace rollup)

**Exit:** error-handling cluster parity green; unified quality report matches elicit_doc resolution order.

### Phase P3 — Antipatterns + tooling

- [x] Antipatterns etiquette (5 rules incl. contract bounds, version-in-member)
- [x] Tier C amenable antipatterns parity harness (`tests/quality_parity.rs`)
- [x] Tracing auto-apply (`cordial quality --apply`) optional parity with `instrument_apply`
- [x] Exception patch path aliases (`quality/patches/` ↔ `exceptions/`)

### Phase P4 — Coverage pipeline parity (elicitation profile)

**Deferred** — prove the generalized `Coverage` model on framework profiles
(P5 / C5–C6) before investing in elicitation shadow/build parity. See
[coverage-as-plugin.md](coverage-as-plugin.md) priority note.

See [coverage-as-plugin.md](coverage-as-plugin.md) for the target architecture
(`ElicitationCoverage: Coverage + Plugin`).

- [x] `cordial build rustdoc` — cargo rustdoc + cache layout (`cache/builds/`, `cache/rustdoc/`)
- [x] TraitPrereqs + ElicitComplete gap classification (`MissingOurTraits`, `ReadyForElicitComplete`, `FeatureGatedExternal`, `ExternallyBlocked`)
- [x] `gaps-impl.csv` reporter alongside expanded `impl-coverage.csv`
- [x] Proof harness linkage in impl rows — `collect::ProofHarness`, `proof_test` / `composition_test` on `impl-coverage.csv`
- [x] Full feature probes (`FeatureGatedExternal` gap kind) — `collect::TypeFeatureProbe`, dep feature unlock hints, optional probe rustdoc cache (`CORDIAL_PROBE_FEATURES=1`)
- [x] Trenchcoat wrapper-coverage integration — `WrapperCoverageMap` from hub `From<T>` pairs; indirect coverage in `assess_impl_gap`
- [x] Shadow pipeline: cross-crate mirror compare — `shadow::build_shadow_report`, `gaps-shadow.csv`, `shadow-{upstream}.csv` (`tests/shadow_pair_gaps.rs`)
- [x] `TRACKED_TARGETS` / `TargetProvider` roster (`ELICITATION_TRACKED_TARGETS`, `ElicitationTargetProvider`)
- [x] Coverage `summary.md` metric parity — impl + shadow tables match elicit_doc columns (`tests/elicitation_coverage_summary.rs`)
- [x] Tier A `minimal-workspace` coverage parity harness — `tests/elicitation_parity.rs` (url-scoped `gaps-impl.csv`, `gaps-shadow.csv`, `shadow-url.csv` vs frozen baselines; refresh via `elicitation_parity_refresh`)

**Exit (revised):** elicitation parity after C5–C6 framework std parity green.

### Phase P5 — Framework profiles + delegation (**current focus**)

See [coverage-as-plugin.md](coverage-as-plugin.md) phases C5–C6
(`HomecomingStdCoverage`, `AmenableStdCoverage`).

- [x] `framework_std` module — merged std inventory, trait report, gaps (port from `elicit_doc`)
- [x] `HomecomingStdCoverage` plugin — `Code` trait, `homecoming_core` impl crate
- [x] `AmenableStdCoverage` plugin — registry dump + witness layers
- [x] Unit parity: `amenable_registry_test` semantics (`tests/amenable_registry.rs`)
- [x] Sysroot rustdoc cache — `~/.cordial/sysroot/cache/rustdoc/{std,core,alloc}.json`; `cordial build sysroot`
- [x] Tier C workspace parity: `homecoming/`, `amenable/` vs `elicit_doc run` — harness in `tests/coverage_parity.rs` (`PARITY_TIER_C=1`)
- [x] Tier C quality parity: amenable antipatterns + version-in-member — harness in `tests/quality_parity.rs` (`PARITY_TIER_C=1`)
- [ ] `cordial_homecoming` / `cordial_amenable` re-export crates (optional)
- [x] Snapshot/digest parity (`shadow-core-support`, roster digests) or documented replacements

**Exit:** `elicit_doc run` and `cordial coverage` produce equivalent framework std
reports on homecoming and amenable workspaces.

---

## Cache mapping

Parity tests focus on **user-facing reports**, not byte-identical cache trees.
When comparing cache JSON:

| elicit_doc cache | Cordial equivalent | Compare strategy |
| --- | --- | --- |
| `{crate}.panics.scan.json` | IR + findings (or future scan cache) | Normalize to `(kind, file, line, context)` set |
| `inventories/member-{crate}.inventory.json` | `cache/{crate}.ir.json` Item nodes | Type path set + `Implements` edges |
| `assessed/impl-gaps.json` | impl-coverage findings export | Gap kind + type_path + missing traits |
| `extracts/proof-harness.json` | _(planned enricher output)_ | Test status per type path |
| `manifest.json` + build artifacts | `{crate}.ir.digests.json` | Fingerprint fields present; incremental skip behavior tested separately |
| `cache/rustdoc/shadow-dep-{shadow}-{upstream}.json` | same path under store | Inventory type-path set for upstream built with shadow dep features |

---

## Open questions

1. **Fixture ownership** — symlink `elicit_doc/tests/fixtures` into cordial, or copy minimal subsets?
2. **Store path unification** — should cordial adopt `quality/` + `coverage/` subdirs for drop-in diff?
3. **Baseline churn** — policy for updating baselines when elicit_doc changes (lock to rev in MANIFEST).
4. **Nightly Tier C** — run parity against live `elicitation` on schedule vs release gate only.
5. **Strict vs fuzzy line numbers** — allow ±1 when macro expansion differs?

---

## References

- [CORDIAL_PLAN.md](../../CORDIAL_PLAN.md) — architecture and build phases 0–6
- [elicit_doc PLANNING_INDEX](../../../elicit_doc/PLANNING_INDEX.md)
- [elicit_doc project store layout](../../../elicit_doc/src/project.rs)
- [elicit_doc cache layout](../../../elicit_doc/src/pipeline/layout.rs)
- [elicit_doc quality all](../../../elicit_doc/src/quality/all.rs)

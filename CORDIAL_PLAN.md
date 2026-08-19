# CORDIAL_PLAN.md

## Goal

`cordial` is a refinement of [`elicit_doc`](../elicit_doc): **polite standards for
code development**. It produces local, regeneratable reports about whether a
codebase follows the etiquettes you care about. Artifacts land under
`~/.cordial/{project}/` and are never committed to git.

Where `elicit_doc` is a monolith — hardcoded pipelines, closed rule enums,
~30 baked-in tracked targets, three parallel orchestration paths for
impl/shadow/framework modes — `cordial` is a **plugin framework**. Users
implement trait interfaces at well-defined seams, register **etiquettes**
(named bundles of hooks), and run them against a shared intermediate
representation of the code under analysis.

The motivating use case is twofold:

1. **Port elicit_doc's coverage and quality analyses** as first-party
   etiquettes built on cordial's core, gaining deduplicated parsing and a
   composable architecture.
2. **Enable third-party and project-local lints** without forking cordial —
   impl `Probe` (and optionally `Assessor`, `Reporter`) in your own crate,
   register an etiquette, and run it alongside built-in checks.

## Status

Phases 0–6 implemented on `main`. Built-in etiquettes cover panics, tracing,
and elicitation coverage (impl / trenchcoat / shadow). The `cordial` binary
drives session runs against local and workspace projects.

Output parity with `elicit_doc` is tracked in
[docs/planning/elicit-doc-parity.md](docs/planning/elicit-doc-parity.md).
Post-parity structural alignment onto hook seams is tracked in
[docs/planning/post-parity-alignment.md](docs/planning/post-parity-alignment.md).

## Relationship to elicit_doc

`elicit_doc` today runs two monolithic pipeline families:

| Family | Input | Acts |
| --- | --- | --- |
| **Coverage** | rustdoc JSON + cargo builds | Build → Inventory → Extract → Assess → Report → Summarize (impl + shadow passes) |
| **Quality** | syn scans of `src/**/*.rs` | Scan → Assess → Write (per scanner: tracing, derives, panics, antipatterns, error-sites, modularity, …) |

Each quality scanner re-implements file walking, `syn::parse_file`, module
path resolution, and visitor state. Coverage collectors re-walk the same
rustdoc-derived inventory. Rule IDs are closed enums; adding a lint
requires a code change and release.

`cordial` preserves the *outputs* and *workflow* (local store, CSV +
checklists, JSON patch exceptions, stage-level caching) while replacing
the *structure* with composable hooks around a shared graph IR.

`elicit_doc` becomes a consumer of `cordial` coverage plugins — not the
framework itself.

## Naming

| Term | Meaning |
| --- | --- |
| **cordial** | The crate / tool. Polite standards for code development. |
| **Etiquette** | A named bundle of hooks (loaders, enrichers, probes, assessors, reporters). *Not* `Standard` — that name is taken by `amenable_core::Standard`, a provenance-root role in the amenable ecosystem. |
| **Probe** | Walks the IR and attaches **markers** (observations, not verdicts). |
| **Assessor** | Consumes markers and emits **findings** (judged issues). |
| **Reporter** | Renders findings into **artifacts** (CSV, checklist, summary). |

### Cargo features (built-in plugins)

| Feature | Plugin |
| --- | --- |
| `panics` | Panic sources — error-handling layer (library → internal errors, binary/tests → miette) |
| `tracing` | `#[instrument]` coverage → classified recipes ([tracing-etiquette.md](docs/planning/tracing-etiquette.md)) |
| `derives` | Manual builder/getter/setter/new patterns |
| `allows` | `#[allow(...)]` inventory |
| `modularity` | File/function size, types-per-file, module-size σ, hierarchy lints |
| `quality` (default) | All source-quality scanners (panics, tracing, error stack, derives, allows, modularity, antipatterns, cfg_scatter, visibility, cli_layout, glob_imports, inline_tests) |
| `impl_coverage` | Trait impl coverage (requires `rustdoc`) |
| `trenchcoat` | Trenchcoat wrapper coverage |
| `shadow` | Shadow mirror coverage |
| `elicitation` | All coverage plugins (`impl_coverage`, `trenchcoat`, `shadow`) |
| `full` | Every built-in plugin |

Downstream: enable `cordial/elicitation` (and `cli` for the binary). Default
features are `quality` and `cli`.

Usage: *"run the panics etiquette"*, *"register a custom etiquette"*.

## Design principles

1. **Trait interfaces at every seam** — `Finding`, `Marker`, `Rule`,
   `Artifact`, `IrView`, and related types are traits, not concrete structs.
   Users swap representations over time; cordial core ships default impls
   internally but plugins never depend on them.

2. **Graph IR as the center of gravity** — code is read into a
   `petgraph`-backed intermediate representation once, enriched
   incrementally, probed many times. Probes do not parse source.

3. **Markers ≠ findings ≠ artifacts** — observations, judgments, and
   rendered output are separate data classes with separate producers.

4. **Overbuilt graph, lean query** — `StableDiGraph` + indexes +
   purpose-built `Query` trait covers probe needs. External databases
   (SurrealDB, FalkorDB) are optional view/export layers, not the
   authoritative IR.

5. **Minimize scope in v0.1** — prove the architecture with one ported
   etiquette before migrating all of elicit_doc.

---

## Architecture overview

Every analysis run follows three acts:

```
Read code into IR  →  Flag things of interest  →  Report
       ↑                        ↑                      ↑
   Load + Enrich            Probe + Assess          Reporter
```

Etiquettes declare which hooks participate at each act. The session
deduplicates loaders and enrichers across etiquettes, builds IR once, runs
all probes, routes markers to assessors, and renders through reporters.

```
┌──────────────────────────────────────────────────────────────┐
│  Session                                                     │
│                                                              │
│  Etiquette A ──┐                                             │
│  Etiquette B ──┼──► Load (deduped)                           │
│  Etiquette C ──┘         │                                   │
│                          ▼                                   │
│                    Build + Enrich IR  (petgraph, cached)     │
│                          │                                   │
│              ┌───────────┼───────────┐                       │
│              ▼           ▼           ▼                       │
│           Probe A     Probe B     Probe C                    │
│              │           │           │                       │
│              └───────────┼───────────┘                       │
│                          ▼                                   │
│                       Markers                                │
│                          │                                   │
│              ┌───────────┼───────────┐                       │
│              ▼           ▼           ▼                       │
│         Assessor A  Assessor B  Assessor C                   │
│              │           │           │                       │
│              └───────────┼───────────┘                       │
│                          ▼                                   │
│                       Findings                               │
│                          │                                   │
│              ┌───────────┼───────────┐                       │
│              ▼           ▼           ▼                       │
│         Reporter A  Reporter B  Rollup                       │
│                          │                                   │
│                          ▼                                   │
│                      Artifacts                               │
└──────────────────────────────────────────────────────────────┘
```

---

## Hook seams

### 1. Loader — read raw material

```rust
pub trait Loader: Send + Sync {
    fn id(&self) -> &str;
    fn load(&self, session: &dyn Session, target: &dyn CrateTarget) -> Result<Box<dyn LoadView>>;
}
```

Built-in loaders:

| Loader | Produces |
| --- | --- |
| `SourceLoader` | Parsed syn AST material for `src/**/*.rs` |
| `RustdocLoader` | rustdoc JSON for a crate |
| `CargoLoader` | Workspace metadata, member list, dependencies |

Loaders produce opaque bundles. They do not emit findings.

### 2. IrEnricher — extend the IR

```rust
pub trait IrEnricher: Send + Sync {
    fn id(&self) -> &str;
    fn enrich(&self, ir: &mut dyn IrMut, load: &dyn LoadView, session: &dyn Session) -> Result<()>;
}
```

Enrichers add nodes, edges, and attributes to the graph. They build facts,
not judgments. Examples:

| Enricher | Adds |
| --- | --- |
| `PathIndexEnricher` | `by_path` index entries |
| `ScopeEnricher` | `Scope` edges (expr → enclosing fn/impl/module) |
| `SynDocLinkEnricher` | links syn Item nodes ↔ rustdoc Item nodes by qualified path |
| `TraitImplEnricher` | `Implements` edges from rustdoc |
| `TrenchcoatEnricher` | `Wraps` edges from `From<Foreign> for Wrapper` impls |
| `ShadowLinkEnricher` | `Mirrors` edges (shadow ↔ upstream, cross-crate) |
| `ErrorFlowEnricher` | `ErrorFlow` edges for error-site analysis |

### 3. Probe — flag what you care about

```rust
pub trait Probe: Send + Sync {
    fn id(&self) -> &str;
    fn interests(&self) -> &dyn Query;
    fn probe(&self, ir: &dyn IrView, session: &dyn Session) -> Result<Vec<Box<dyn Marker>>>;
}
```

Probes attach **markers** to IR nodes. A marker is an observation, not a
verdict.

```rust
pub trait Marker: Send + Sync {
    fn probe(&self) -> &str;
    fn label(&self) -> &str;
    fn anchor(&self) -> &dyn IrAnchor;
    fn span(&self) -> Option<&dyn SourceSpan>;
}
```

Each probe defines its own marker type (`PanicMarker`, `MissingTraitMarker`).
Assessors select markers by label.

### 4. Assessor — turn markers into findings

```rust
pub trait Assessor: Send + Sync {
    fn id(&self) -> &str;
    fn consumes(&self) -> &[&str];   // marker labels

    fn assess(
        &self,
        markers: &[&dyn Marker],
        ir: &dyn IrView,
        session: &dyn Session,
    ) -> Result<Vec<Box<dyn Finding>>>;
}
```

Assessment is where judgment happens: severity, gap classification,
exception/patch suppression, cross-marker joins (error-sites phase 3
needs markers from phases 1 and 2).

Assessors may also use associated types for fully typed pipelines within a
single etiquette; the session type-erases at the composition boundary.

### 5. Reporter — render findings

```rust
pub trait Reporter: Send + Sync {
    fn id(&self) -> &str;
    fn render(
        &self,
        findings: &[&dyn Finding],
        ir: &dyn IrView,
        session: &dyn Session,
    ) -> Result<Vec<Box<dyn Artifact>>>;
}
```

Reporters produce **artifacts** — rendered files, not live data.

---

## Core object traits

### Finding

```rust
pub trait Finding: Send + Sync {
    fn rule(&self) -> &dyn Rule;
    fn disposition(&self) -> Disposition;
    fn anchor(&self) -> &dyn IrAnchor;

    /// Reporters pull structured fields without knowing the concrete type.
    fn emit(&self, sink: &mut dyn FindingSink);
}
```

`FindingSink` is the format-evolution point. A v1 finding emits `rule_id`,
`file`, `line`, `message`. A v2 finding adds `suggested_fix`, `related_types`.
Old reporters skip unknown fields; new reporters consume them. Users replace
the finding shape entirely by impling `Finding` on their own type.

### Rule

```rust
pub trait Rule {
    fn id(&self) -> &str;
    fn category(&self) -> &str;
    fn description(&self) -> &str;
}
```

Closed enums in elicit_doc (`PanicSourceKind`, `DeriveRuleId`) become
user-defined `Rule` impls.

### Artifact

```rust
pub trait Artifact {
    fn name(&self) -> &str;
    fn media_type(&self) -> &str;
    fn write_to(&self, dest: &mut dyn Write) -> Result<()>;
}
```

---

## Etiquette

An etiquette is a named declaration of which hooks participate:

```rust
pub trait Etiquette {
    fn id(&self) -> &str;
    fn name(&self) -> &str;

    fn loaders(&self) -> &[&dyn Loader];
    fn enrichers(&self) -> &[&dyn IrEnricher];
    fn probes(&self) -> &[&dyn Probe];
    fn assessors(&self) -> &[&dyn Assessor];
    fn reporters(&self) -> &[&dyn Reporter];
}
```

Example — panics etiquette (ported from elicit_doc):

```
Etiquette "panics"
  loaders:    [SourceLoader]
  enrichers:  [ScopeEnricher]
  probes:     [PanicSiteProbe]
  assessors:  [PanicAssessor]
  reporters:  [CsvReporter, ChecklistReporter]
```

Example — impl coverage etiquette:

```
Etiquette "impl-coverage"
  loaders:    [RustdocLoader]
  enrichers:  [TraitImplEnricher, TrenchcoatEnricher, ProofHarnessEnricher]
  probes:     [MissingPrereqProbe, UncoveredTypeProbe]
  assessors:  [ImplGapAssessor]
  reporters:  [ImplCoverageReporter, ImplChecklistReporter]
```

A third-party user registers an etiquette with one probe and one reporter,
inheriting shared loaders and enrichers from session defaults.

---

## Session

```rust
pub trait Session {
    fn register(&mut self, etiquette: &dyn Etiquette);
    fn run(&self, filter: &dyn RunFilter) -> Result<Box<dyn RunOutcome>>;
}

pub trait RunOutcome {
    fn findings(&self) -> dyn Iterator<Item = &dyn Finding>;
    fn artifacts(&self) -> dyn Iterator<Item = &dyn Artifact>;
}
```

The session owns:

- store layout (`~/.cordial/{project}/`)
- cache invalidation and fingerprints
- loader deduplication across etiquettes
- IR build → enrich → probe → assess → report ordering
- assessor dependency resolution (marker label availability)

---

## IR design

### Why petgraph

The IR is a **directed graph** backed by `petgraph::StableDiGraph`:

- Enrichers add nodes and edges incrementally; indices must remain stable.
- Probes and markers hold `NodeIndex` anchors that survive further enrichment.
- Coverage, error chains, shadow diffs, and trait closure are graph
  problems — path queries, anti-joins, subgraph comparison.
- `petgraph` algorithms (DFS, BFS, simple paths, connected components) cover
  probe needs without an external query engine.

`StableDiGraph` over plain `DiGraph` because enrichment is append-heavy and
probe anchors must not invalidate mid-run.

### Scope: per-crate graph, workspace shell

```
WorkspaceIr
├── CrateIr   (StableDiGraph + indexes)   ← one per workspace member
├── CrateIr
└── cross_crate edges                     ← Dep, Mirrors, Wraps
```

Per-crate graphs are the natural cache unit (matches elicit_doc's per-crate
inventory/scan caches). Probes default to a single `CrateIr`. Assessors
doing shadow diffs or cross-crate joins query the workspace shell.

**Workspace-level data** (not nodes on a merged graph):

| Field / attr | Source | Role |
| --- | --- | --- |
| `cross_crate_edges` | shadow preload, trenchcoat | `Mirrors`, cross-crate `Wraps` |
| `wrapper_coverage_map` | hub IR query (`impl_coverage`) | foreign type → elicitation wrappers |
| Rustdoc facts on type nodes | `RustdocStructureEnricher` | methods, impls, prereqs, trenchcoat — see `src/ir/attrs.rs` |

`parse_rustdoc_json` runs only in `RustdocLoader`, on-demand crate preload
(`ir/crate_load.rs`), probe/sysroot build paths, and `cordial::testing` oracles.
Session hot paths never retain `RustdocInventory`.

The graph is **append-only** in v0.1 — no node deletion. Simplifies index
stability and cache serialization.

### Node taxonomy

| Kind | Source | Role |
| --- | --- | --- |
| `Workspace`, `Crate`, `Module` | cargo + syn | Structural hierarchy |
| `Item` (fn, struct, enum, trait, …) | syn + rustdoc | Primary anchor for coverage |
| `ImplBlock`, `ImplItem`, `Field`, `Variant` | syn | Inner structure |
| `Expr`, `Pat`, `Type` | syn (lazy) | Deep probes: panics, `?`, unwrap |
| `Attribute` | syn | `#[allow]`, `#[instrument]`, etc. |
| `Plugin(…)` | enricher-defined | Extension without polluting the enum |

Expr nodes are **lazy-expanded** — only materialized when a probe's `Query`
requests them. Coverage-only runs that never touch syn exprs avoid the cost.

### Edge taxonomy

| Edge | From → To | Source |
| --- | --- | --- |
| `Contains` | Module → Item, Item → inner | syn loader |
| `Defines` | Crate → Module, ImplBlock → ImplItem | syn loader |
| `Scope` | Expr → enclosing Fn/Impl/Module | scope enricher |
| `Implements` | Type → Trait | rustdoc enricher |
| `Aliases` | TypeAlias → Type | rustdoc enricher |
| `Reexports` | Module → Item | rustdoc enricher |
| `Wraps` | Wrapper → Foreign | trenchcoat enricher |
| `Mirrors` | Shadow Item → Upstream Item | shadow enricher (cross-crate) |
| `Depends` | Crate → Crate | cargo loader |
| `ErrorFlow` | Expr → Expr/Type | error-site enricher |
| `HasAttr` | Item/Fn → Attribute | syn loader |
| `Plugin(…)` | enricher-defined | custom enrichers |

### Internal shape (hidden behind IrView / IrMut)

```rust
pub struct CrateIr {
    graph: StableDiGraph<NodeWeight, EdgeWeight>,
    indexes: IrIndexes,
}

struct IrIndexes {
    by_path: HashMap<QualifiedPath, NodeIndex>,
    by_span: SpanIndex,
    by_kind: HashMap<NodeKind, Vec<NodeIndex>>,
    by_attr: HashMap<AttrKey, Vec<NodeIndex>>,
}
```

Public API:

```rust
pub trait IrView {
    fn walk(&self, query: &dyn Query) -> dyn Iterator<Item = NodeRef>;
    fn node(&self, id: NodeId) -> Option<NodeRef>;
    fn edges_from(&self, id: NodeId, kind: EdgeKind) -> dyn Iterator<Item = (EdgeRef, NodeRef)>;
    fn edges_to(&self, id: NodeId, kind: EdgeKind) -> dyn Iterator<Item = (EdgeRef, NodeRef)>;
}

pub trait IrMut {
    fn insert_node(&mut self, kind: NodeKind, span: Option<SourceSpan>) -> NodeId;
    fn insert_edge(&mut self, from: NodeId, to: NodeId, kind: EdgeKind) -> EdgeId;
    fn set_attr(&mut self, node: NodeId, key: AttrKey, value: AttrValue);
}
```

`NodeId` wraps `NodeIndex`; petgraph stays an implementation detail.

Enrichers attach typed payloads via `AttrStore` on nodes; probes read
through `NodeView::attribute(key)`.

---

## Query layer

Probes declare interest through a `Query` trait; they do not call petgraph
directly.

```rust
pub trait Query {
    fn node_kinds(&self) -> &[NodeKind];
    fn edge_kinds(&self) -> &[EdgeKind];
    fn predicate(&self) -> Option<&dyn NodePredicate>;
}
```

Built-in query constructors:

```rust
Query::nodes(Item)
Query::exprs_with_macros()
Query::types_lacking_edge(EdgeKind::Implements, trait_path("Serialize"))
Query::path(from, via: ErrorFlow, to)
Query::neighbors(node, Scope).first()
```

Implementation: `IrIndexes` for O(1) path/kind lookups; petgraph `Dfs`,
`Bfs`, `all_simple_paths` for traversals; set difference for anti-joins.

This covers all elicit_doc query patterns without SurrealQL or Cypher.

### Why not SurrealDB or FalkorDB as the core IR

| Option | Verdict |
| --- | --- |
| **petgraph + Query** | **Primary IR.** In-process mutation, stable indices, zero deps, trait-friendly probe/enricher integration. |
| **SurrealDB** | **Optional export layer.** Already in the elicitation ecosystem (`elicit_surrealdb`). Good for agent/MCP interactive queries and cross-run history. Wrong for the enricher/probe hot loop — every edge insert becomes a query round-trip. |
| **FalkorDB** | **Not now.** "Embedded" still spawns `redis-server` + loads `falkordb.so`. Server process overhead for a derived analysis artifact. Revisit for an interactive Cypher REPL over code graphs if that product surface emerges. |

Optional future path:

```
cordial run  →  petgraph IR  →  cache to disk
                    ↓ optional
cordial explore  →  materialize into SurrealDB  →  interactive / agent queries
```

---

## Caching

```
~/.cordial/cordial.toml            ← user-global etiquette thresholds
{workspace}/cordial.toml           ← project overrides (wins)
~/.cordial/{project}/
├── cache/{crate}/ir.json          ← serialized graph + indexes
├── cache/{crate}/ir.digests       ← source + rustdoc + enricher fingerprints
├── findings/                      ← per-etiquette outputs
└── exceptions/                    ← JSON patch suppressions (ported from elicit_doc)
```

Cache key includes:

- source file fingerprints
- rustdoc JSON fingerprint (when loaded)
- enricher set + versions

If only probes change, reload cached IR and re-probe. If an enricher
changes, re-enrich from raw loader output without re-parsing.

---

## What elicit_doc maps to

| elicit_doc today | cordial |
| --- | --- |
| Flat `Inventory.items` | Item nodes + `Contains` / `Implements` edges |
| Per-scanner syn visitors with fn/impl stacks | `Scope` edges + shared `ScopeEnricher` |
| `HashMap<String, TraitPrereqs>` | `Implements` edge queries per type node |
| Error-sites multi-phase join | `ErrorFlow` edges + assessor dependency on marker labels |
| Shadow diff by comparing inventories | `Mirrors` subgraph comparison |
| Trenchcoat pair collection | `Wraps` edges |
| Re-parsing every file per scanner | Parse once, probe many |
| Closed rule enums | Open `Rule` trait |
| `run_quality_all` manual assembly | `Session::run` over registered etiquettes |
| Hardcoded `TRACKED_TARGETS` | `TargetProvider` + `Coverage::targets` — see [coverage-as-plugin.md](docs/planning/coverage-as-plugin.md) |
| `~/.elicit_doc/` store | `~/.cordial/` store |

---

## Ecosystem placement

| Crate | Relationship |
| --- | --- |
| **elicit_doc** | Predecessor; logic migrates into cordial etiquettes |
| **amenable** | `Standard` name avoided; cordial etiquettes may *check* amenable patterns but do not use amenable's `Standard` trait |
| **homecoming** | Also uses `petgraph` for expression IR (`homecoming_core::Ir`); different domain — cordial's graph is workspace-scale analysis, not code capture |
| **elicitation / elicit_surrealdb** | Optional SurrealDB export for agent-facing graph queries |

Dependency direction: `cordial` is standalone. Domain-specific etiquette
crates (`cordial` first-party etiquettes, user crates) depend on `cordial`, not the
reverse.

---

## Proposed crate layout

```
cordial/                  # one package: library + `cordial` binary
examples/custom_plugins/  # downstream plugin template
```

Users depend on `cordial` and may publish their own plugin crates. First-party
`cordial_*` satellites are not used. CLI types and dispatch live in the
library; `src/main.rs` parses, calls `Cli::act`, and surfaces with miette.
See [one-crate-cli-layout.md](docs/planning/one-crate-cli-layout.md).

---

## Build order

### Phase 0 — Scaffold (current)

- [x] Initialize repo, `main` branch
- [x] Write this planning document

### Phase 1 — Core traits + IR

- [x] `Session`, `Etiquette`, and hook seam traits (`Loader`, `IrEnricher`, `Probe`, `Assessor`, `Reporter`)
- [x] Object traits (`Finding`, `Marker`, `Rule`, `Artifact`, `IrView`, `IrMut`, `IrAnchor`, `SourceSpan`, `FindingSink`)
- [x] `CrateIr` with `StableDiGraph`, `IrIndexes`, append-only mutation
- [x] `Query` trait + basic constructors
- [x] `SourceLoader` + `ScopeEnricher`
- [x] Store layout (`~/.cordial/{project}/`) and IR cache read/write

### Phase 2 — Reference etiquette

- [x] Port elicit_doc **panics scanner** as the first etiquette
      (`PanicSiteProbe`, `PanicAssessor`, CSV + checklist reporters)
- [x] Prove: load once → enrich → probe → assess → report
- [x] Integration test against a small fixture crate

### Phase 3 — Second etiquette + cross-cutting

- [x] Port elicit_doc **tracing instrument** scanner as second etiquette
      (exercises `Attribute` nodes + `HasAttr` edges)
- [x] Multiple etiquettes in one run with deduplicated loading
- [x] JSON patch exception filtering

### Phase 4 — Coverage plugins

Scaffold complete (rustdoc hooks, impl/trenchcoat/shadow etiquettes, build stage).
Refactor to **Plugin + Coverage** model tracked in
[docs/planning/coverage-as-plugin.md](docs/planning/coverage-as-plugin.md).

- [x] `RustdocLoader`, `TraitImplEnricher`, impl coverage etiquette
- [x] Shadow / trenchcoat etiquettes
- [x] `cordial build rustdoc` + TraitPrereqs / gap kinds
- [x] Begin `cordial_elicitation` crate
- [ ] `Plugin` registration seam + `Coverage` supertrait
- [ ] `ElicitationCoverage` concrete profile
- [ ] `HomecomingStdCoverage` / `AmenableStdCoverage`

### Phase 5 — CLI + polish

- [x] `cordial_cli`: `run`, `quality`, `coverage`, `view`, `exceptions`
- [x] Rollup reporter (executive summary across etiquettes)
- [x] Optional SurrealDB export for agent integration

### Phase 6 — Workspace + cache digests

- [x] `cargo metadata` workspace member discovery (`discover_crate_targets`)
- [x] Per-crate probe, assess, and exception filtering in session runs
- [x] IR cache digest files (`{crate}.ir.digests.json`)

---

## Open questions

1. **Assessor typing** — support both associated types (typed within one
   etiquette) and type-erased (`Box<dyn Finding>`) at the session boundary
   from the start, or pick one for v0.1?

2. **IrMut scope** — full graph access for enrichers, or constrained
   operations (`insert_node`, `insert_edge`, `set_attr` only)?

3. **Marker routing** — label-based (`fn label() -> &str`) sufficient for
   cross-probe joins, or typed subscription mechanism needed early?

4. **AttrStore typing** — string keys with JSON values (flexible) vs typed
   plugin registry (safer)?

5. **Cache serialization format** — JSON (debuggable, matches elicit_doc)
   vs bincode (compact)?

6. **Expr lazy expansion trigger** — probe declares interest and loader
   expands on demand, or enricher pass expands all expr subtrees for
   requested crates?

7. **Auto-fix** — elicit_doc only auto-fixes tracing `#[instrument]`.
   Worth a `FixAction` trait on probes/assessors from the start, or defer?

8. **Config file** — compile-time etiquette registration only, or
   `cordial.toml` listing plugin crates at runtime?

---

## References

- [`elicit_doc`](../elicit_doc) — predecessor tool
- [`elicit_doc` PLANNING_INDEX](../elicit_doc/PLANNING_INDEX.md) — quality scanner plans
- [`homecoming` HOMECOMING_PLAN](../homecoming/HOMECOMING_PLAN.md) — trait-first design precedent
- [`amenable_core::Standard`](../amenable/crates/amenable_core/src/roles.rs) — name collision avoided
- [`homecoming_core::Ir`](../homecoming/crates/homecoming_core/src/ir.rs) — petgraph precedent in ecosystem

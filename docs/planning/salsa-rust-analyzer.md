# A shared semantic backend with Salsa and rust-analyzer

**Status:** Adoption in progress. Salsa and rust-analyzer are the selected
source backend. Migrate existing collection through the shared loader and
retire its `syn` ownership.
Research and dependency compatibility checked on 2026-10-30.

## Purpose

Strategy transformations repeatedly analyze closely related programs. A return
type changes, its callers need adaptation, another error source appears, and
the next candidate needs analysis and compilation. Cordial should concentrate
on constructing the selected design, preserving evidence, and validating the
result. Implementing Rust name resolution, type inference, and invalidation
ourselves consumes effort that could go toward those transformations.

Replace the source frontend with rust-analyzer and extend its
Salsa database with Cordial queries. Keep one canonical analysis backend and
the shared IR boundary. Preserve the existing transformation operations,
evidence requirements, transaction handling, and executable specimens.

This migration supports the [strategy transformation plan](strategy-transformation.md).
It pauses further composition of nested error migrations while we migrate the
frontend they consume. It does not discard completed transformations.

## What exists today

| Responsibility | Current implementation | Consequence for a pivot |
| --- | --- | --- |
| Source ownership and parsing | `src/loader/database/`, `source.rs`, `source/cache.rs`, `source_work.rs` | Source sessions now register canonical text inputs in the database. Existing collectors still consume one cached `syn::File` per path until their syntax migration. |
| Syntax collection | `src/loader/populate.rs` and etiquette collectors | Collectors depend on `syn` nodes and visitors. Replacing the frontend requires migrating them. |
| Shared evidence | `src/ir/`, `src/session/source_facts.rs` | Keep the `IrView` consumption boundary and typed source, call, and error-family facts. |
| Direct call resolution | `src/enricher/source_calls.rs` | Currently restricts resolution to unique local names in a trusted module and refuses several generic, macro, and attribute boundaries. |
| Transformation planning | `src/transform/` | Retain policy, error-family layout, guarded edits, and composition; change how semantic facts are supplied. |
| Source writes | `src/source_write.rs` | Retain snapshot checks, staging, and rollback; analysis revisions alone do not provide a filesystem transaction. |
| Compilation evidence | `tests/support/strategy_specimens.rs` | Executable specimens invoke Cargo and validate warning-free builds and behavior. The closed panic writer does not itself run Cargo. |

The frontend migration is substantial. Converting the existing shared tree to
rust-analyzer by printing and reparsing it would defeat the invariant. Running
both frontends over every production input would also leave duplicate work.

## What upstream provides

rust-analyzer already represents analysis in a Salsa `RootDatabase`. Its
`load-cargo` implementation explicitly supports loading into an existing
database for third-party extensions. Cordial extends that same database with
tracked queries. The source loader
owns the database adapter in `src/loader/database/`; downstream consumers read
the typed projection through `IrView`.

Sources: [RootDatabase implementation](https://raw.githubusercontent.com/rust-lang/rust-analyzer/master/crates/ide-db/src/lib.rs),
[Cargo loader implementation](https://raw.githubusercontent.com/rust-lang/rust-analyzer/master/crates/load-cargo/src/lib.rs).

Its `hir::Semantics` API exposes expression types, callable resolution, method
resolution, and source mappings. These are better starting points for our
semantic facts than matching printed paths. The APIs return optional results:
an unavailable answer must remain an explicit gap in Cordial evidence.

Source: [Semantics implementation](https://raw.githubusercontent.com/rust-lang/rust-analyzer/master/crates/hir/src/semantics.rs).

Salsa supplies dependency tracking, memoization, and invalidation as declared
inputs change. Deterministic queries read those inputs; the outer driver owns
input changes and effects. It does not supply Cordial's strategy decisions or
prove that an edited program compiles.

Source: [Salsa overview](https://salsa-rs.github.io/salsa/overview.html).

## Recommended ownership

```text
Workspace driver: source, Cargo configuration, build outputs, selected policy
                              |
                              v
         One rust-analyzer / Salsa database
         canonical syntax -> semantic identities and types
                          -> Cordial collection/enrichment queries
                          -> shared IR facts
                              |
                              v
            IrView -> findings and transformation plans
                              |
                              v
           Candidate revision -> analysis -> Cargo validation
                              |
                              v
                  accept and write / reject
```

### rust-analyzer owns Rust language analysis

Use its syntax and semantic layers directly through a private adapter. Prefer
typed `hir` facts to extracting types from hover strings or making a series of
LSP requests. Import declarations, instantiated call contracts, type arguments,
source locations, and resolution gaps into the shared IR.

Use item identity to distinguish the standard `Result`, an alias of it, and an
unrelated type with the same spelling. Track declared generic contracts and
concrete substitutions separately. A generic trait method identifies a
contract; it does not necessarily identify one concrete runtime target.

### Salsa owns derived computation

Extend the same database with deterministic Cordial queries where the chosen
release permits it. Start with function facts, resolved calls, and error-family
summaries. Keep the existing recursive-family algorithm's explicit fixed-point
semantics; a query engine does not choose the correct treatment of call cycles.

Query at function/module boundaries and compare meaningful outputs. A single
tracked function wrapping the entire existing pipeline would invalidate too
much. Likewise, copying all rust-analyzer facts into a second database after
each edit would require a new invalidation protocol. Reject that design.

The existing `Rc`-backed parsed-source handles and `syn` nodes are not the query
result contract. Prove the selected Salsa release's lifetime and thread-safety
requirements with typed Cordial facts before designing around parallel queries.

### The shared IR remains the consumer contract

The graph may initially be a materialized snapshot of tracked facts. It is a
projection of the canonical database, not another mutable source of truth.
Probes, assessors, reporters, and apply receive `IrView`; they do not keep a
private semantic cache or query rust-analyzer behind the graph's back.

Keep raw rust-analyzer and Salsa handles inside the adapter. Expose typed,
snapshot-bound facts. Database-local identifiers are not durable worklist ids
or proof tokens across revisions, reloads, and processes. Reacquire semantic
handles after changes and bind edit evidence to source digests, target context,
and the relevant analysis revision. Constructors must prevent combining facts
from different revisions.

### Cordial owns strategy and acceptance

Keep choices such as error-family reuse, permitted broadening, Owned/Borrowed
storage, boundary behavior, generated declarations, and transformation ordering
in Cordial. rust-analyzer supplies language facts, not our preferred design.

Cargo/rustc compilation remains required evidence for accepting a candidate.
Successful compilation does not establish behavior preservation: keep the
specimen assertions for values, source chains, caller locations, and boundaries.

## Candidate revision loop

1. Load the workspace and build configuration once for the apply session.
2. Derive a plan entirely from the shared IR, including the caller closure and
   explicit blockers. Preserve current source and manifest fingerprints.
3. Render all related edits in a virtual candidate revision. Change database
   inputs through the driver and reacquire its IR view.
4. Request affected facts and diagnostics. Measure what executes again. Do not
   retain old analysis readers while mutating the database.
5. Materialize a candidate in the approved sandbox and run the appropriate
   Cargo check/build or executable specimen. Reuse its target directory across
   candidates; do not invoke Cargo from a tracked query.
6. Accept only with the required validation evidence and unchanged original
   inputs. Commit through the source transaction. Reject by restoring source
   inputs as a new revision and rebuilding the affected view.

Analysis snapshots share database state; they are not independently writable
branches. Start with serial candidate evaluation. Parallel alternatives need a
separate design and measurements, not an assumption that cloning a database
creates cheap mutable forks.

Sources: [AnalysisHost and Analysis](https://raw.githubusercontent.com/rust-lang/rust-analyzer/master/crates/ide/src/lib.rs),
[Salsa database runtime](https://salsa-rs.github.io/salsa/plumbing/database_and_runtime.html).

Benefits across candidates require keeping the database alive during that
session. Separate CLI invocations do not automatically share its memoized
state. A daemon or persistent database format is a later decision. Compiler
processes also have their own parsing and incremental state; they cannot share
rust-analyzer's tree.

## Invariants and migration risks

- **One parsing owner:** ordinary Rust analysis uses the canonical backend for
  each source revision and parsing context. Consumers cannot create a second
  `syn` tree. New revisions and macro expansions remain backend-owned inputs.
- **Complete inputs:** source text alone is insufficient. Include workspace
  membership, manifests/lockfile, edition, target, cfg/features, toolchain,
  dependency roots, relevant environment, build-script output, and proc macros
  through the workspace driver's supported change paths. Test invalidation;
  do not hide filesystem or environment reads inside supposedly pure queries.
- **Configuration coverage:** semantic evidence is scoped to a crate's selected
  configuration. Whole-source policy lints still need inactive syntax. One
  configuration's call graph cannot establish safety for every feature combo.
- **Incomplete semantics:** dynamic dispatch, indirect calls, unavailable macro
  expansion, dependency boundaries, and unknown types retain explicit gaps.
  Reference search is evidence to inspect, not proof that a library has no
  external callers. API-changing migrations still require a closed boundary.
- **Macro edits:** record expansion origin and original-source mappings. A
  resolved generated call is not automatically an editable source range.
- **Proof dialects:** inventory current Verus parsing and Rust macro islands.
  Prove an upstream extension route or a shared token-based collection design
  before retiring their existing support. Never silently fall back to a second
  whole-file Rust parse when a collector needs a missing fact.
- **Syntax preservation:** preserve comments, attributes, derives, lifetimes,
  and exact source mapping. Replace remaining `syn`/`quote`-dependent analysis
  and edit validation deliberately; inventory generation-only uses separately.
- **API churn and cost:** isolate unstable upstream APIs and measure dependency
  build time, binary size, cold project load, and peak memory. Pin compatible
  upstream crate versions together; use the Salsa version they require.

Upstream documents its IDE library API as unstable. Cargo loading may execute
build scripts and start a proc-macro server. These costs belong in the timing
breakdown rather than being assumed free.

Sources: [rust-analyzer architecture](https://rust-analyzer.github.io/book/contributing/architecture.html),
[Cargo loader implementation](https://raw.githubusercontent.com/rust-lang/rust-analyzer/master/crates/load-cargo/src/lib.rs).

## Alternatives considered

| Approach | Benefit | Assessment |
| --- | --- | --- |
| Salsa around our current frontend | Incremental dependencies while retaining collectors | Leaves Rust resolution and inference in our maintenance budget; useful only if embedding proves unsuitable. |
| External rust-analyzer over LSP | Stable protocol and separate process | Useful editor integration; a poor primary fit for typed, revision-bound evidence across the whole transformation graph. |
| Embedded rust-analyzer with Cordial queries in its database | Shared language analysis and tracked policy facts | Selected direction. Requires adapter maintenance and collector migration. |

## Adoption sequence and gates

Integration tests exercise the shared loader's database and its normal IR
boundary. The shipping loader now registers source inputs there.
Existing collectors still need their `syn` compatibility AST until migrated;
that path does not request a second rust-analyzer syntax tree. The parser
cutover must remove that AST and its cache, not add another parse beside it.

### 1. Adopt canonical inputs and establish semantic queries

- [x] Select `ra_ap_*` `0.0.358` with Salsa `0.28.5`. The cohort requires Rust
  1.98; the repository currently runs Rust 1.99. Keep upstream crates in lockstep.
- [x] Place the database under the existing source loader and route shared
  source-session inputs through it.
- [x] Compile Cordial tracked queries in rust-analyzer's database, with typed
  semantic facts projected into `IrView`.
- [x] Verify resolved import aliases, type aliases, methods, shadowed `Result`,
  unresolved calls, transitive invalidation, unrelated cache reuse, and virtual
  rollback. Record actual test outcomes below before claiming this gate.
- [x] Bind semantic projection to revision/source digests and emit ordinary
  `Calls` edges, with unknown/unsupported evidence explicit.

**Gate:** canonical source ownership has started in the production loader;
semantic identity and invalidation have executable regression coverage. This
is not completion of the parser or collector migration.

### Implementation checkpoint

`SourceDatabase` now lives under `loader/database/`. Each parsed-source session
owns one database and registers captured text before producing the compatibility
tree. Cargo-backed loading uses the same database type and projects semantic
facts into typed IR attributes and ordinary `Calls` edges. Cordial's direct-call
and reachable-error queries execute in rust-analyzer's Salsa database.

`tests/semantic_backend.rs` exercises import aliases, a `Result` type alias,
method resolution, an unrelated enum named `Result`, and unresolved calls. It
changes a leaf's error contract in memory and checks its method, caller, and
caller's caller. Query execution counts distinguish a warm lookup from a
recomputation and verify that the unrelated function stays cached. Restoring
the original input must restore the semantic facts and source digests without
writing the workspace. Const-generic and lifetime-bearing ADTs remain explicit
unsupported types until the projection can retain their arguments.

`tests/shared_source.rs` retains the existing pointer-identity, frozen source
snapshot, reload, and macro-island checks; `tests/source_etiquette.rs` checks
normal source-loader IR population. These remain migration regression tests.

Validation on 2026-10-30: `cargo test --features full --test semantic_backend
--test shared_source --test source_etiquette` passed all four tests without
compiler warnings. The semantic regression completed in 105.44 seconds; this
is the complete test duration, not a parser benchmark. The initial regression
suite also passed with `--no-default-features` before the additional generic
argument assertions were added.

The build-tier submit gate (list 349) cleared all twelve findings introduced
during integration and found no new items. Five prior deep-tier items remain
unverified; the deep tier is reserved for the end of this plan. CI's test job
installs `rust-src` explicitly so the semantic regressions can resolve core and
standard-library definitions.

The production session currently captures text and builds its compatibility
`syn` tree; it does not also request a rust-analyzer parse of that input. Cargo
context loading, remaining syntax collectors, and production semantic enrichment
remain integration work. Reachable error types are an inventory that
includes handled errors. They do not establish which errors escape a function
or authorize transformations.

### Syntax and graph projection checkpoint

The database projection now materializes source modules and recursively nested
items with `Contains` edges, alongside the existing semantic facts and `Calls`
edges. Its private `DatabaseSource` carrier binds source text, digest, canonical
tree, and the database's cached line index. A collector selects its function
from this carrier; it cannot supply an independently parsed tree.

Native collectors populate the existing `SourceModuleScope` and
`FunctionSourceSyntax` contracts: written identifiers and value uses,
signatures, output contracts, calls and their uses, unwrap suffix tokens,
return/success paths, nested scopes, and conservative transformation boundaries.
Function edit contracts remain scoped to top-level free functions, matching the
current consumers. Inactive items retain syntax without acquiring semantic
identities. Syntax recovery errors reject the projection.

`tests/source_backend_parity.rs` compares complete typed payloads from separate
compatibility and database sessions. Its specimen includes doc comments,
comments inside unwrap suffixes, explicit returns, branches, handled calls,
nested method evaluation order, residuals, labeled breaks, lifetimes,
closures, async/const blocks, local items, special
signatures, inactive functions, inline modules, and macros. It also checks
invalid virtual syntax rejection and restoration without filesystem writes.
Collection and graph projection have Hotpath measurement boundaries.

This ports the first collection group into the selected backend. The shipping
source session still uses only its compatibility parse while the remaining
collectors migrate; it does not request both trees. Remove that ownership and
cache together at production cutover.

The call, return, and output modules keep their typed facts separate from the
temporary `compatibility` collectors. Those collectors retain the shipping
behavior and have an explicit removal boundary when the remaining source
collection groups migrate.

Validation on 2026-10-30: `cargo test --features full --test semantic_backend`
passed after graph integration (100.20 seconds). The final focused run,
`cargo test --features full --test source_backend_parity --test shared_source
--test source_etiquette`, passed all four tests without compiler warnings;
the complete parity regression took 92.36 seconds. These are test durations,
not isolated parser timings.
`cordial submit --tier build --deny-open` passed after clearing the new tracing
and modularity findings. The deep tier was not rerun at this checkpoint.

### Collector migration inventory

The starting audit found 53 files consuming `.syntax()` or `.macro_syntax()`
from shared carriers (75 call sites before the new adapter). Port collection
in the following dependency order. Preserve existing fact payloads wherever
possible so consumer stages do not need a simultaneous rewrite.

| Collection group | Current entry points | Required backend capability / retirement |
| --- | --- | --- |
| Source ownership and graph population | `loader/source.rs`, `source/cache.rs`, `source_work.rs`, `populate.rs` | Canonical text inputs first; then syntax carriers, source ranges, item/module population, inactive syntax. Remove `ParsedSource.syntax`, the thread-local AST cache, and the `syn` parser on cutover. |
| Syntax/source facts and direct calls | `ir/function_source.rs`, `ir/source_calls/collect.rs`, `enricher/source_calls.rs` | Signatures, exact edit ranges, declaration identity, instantiated contracts, resolved calls. Replace name-based resolution using semantic facts. |
| Attributes and file policy | `enricher/attribute.rs`; `allows`, `crate_attrs`, `cfg_scatter`, `cfg_hygiene`, `glob_imports`, `inline_tests`, `modularity`, `pageantry` scans | Unexpanded syntax including inactive items, token/range fidelity, nested attributes. Keep normalization and golden findings tests. |
| API and layout | `derives/scan.rs`, `derives/path_inclusion.rs`, `visibility/scan/tree.rs`, `cli_layout/{tree,scan,scan/catalog}.rs` | Item signatures, visibility, derives, `#[path]`, inline/out-of-line modules and Cargo roots. |
| Tracing | `tracing/scan.rs`, `call_graph/collect.rs`, `{subscriber,boundary,print}/scan.rs` | Function/call identity, attributes, macro input syntax, executable boundary facts; retain apply ranges and worklist accounting. |
| Error analysis | `enricher/error/scan.rs`; `error_sites`, `error_chain`, `internal_error_chain/{architecture,type_graph}` | Error/source identity, signatures, conversions, return contracts, owned/borrowed storage and source-chain evidence. |
| Error apply inventory | `internal_error_chain/apply/{inventory,getters,getter_uses,initializers,lifetime,graph_apply}` | One collection of declarations, all uses, initializers, lifetimes, expansion provenance and ranges into existing typed IR. Keep edit rendering text-only. |
| Panic and proof collectors | `panics/{scan,kani_reach}`, `verus_ir/parse.rs`, `antipatterns/{scan,contract_bounds}` | Macro payloads, proof dialect syntax, Kani/Creusot/Verus contracts. No fallback whole-file parser. Resolve the dialect capability before retiring its current support. |
| Candidate validation and generation | source-write/transform validators, test/compliance scan helpers | Separate proposed-output validation and token generation from original-source collection. Port validators deliberately; do not mistake fixture-only parsing for production input ownership. |

Macro islands and snippet parsers need their own inventory in addition to shared
carrier consumers. The audit found 36 files mentioning `syn`/`verus_syn` parse
entry points; some are documentation, fixture helpers, generation, or proposed
edit validation. Classify each before removal. A text search count is not proof
that every production path has been migrated.

Complete the input/context driver before semantic facts authorize transforms:
selected target/features/cfg, build-script outputs, proc macros, manifest and
lockfile identity. Current Cargo loading covers ordinary Rust library fixtures
without running build scripts or proc-macro servers. Missing expansion and
unsupported call/type evidence must stay incomplete.

### 2. Exercise one complete transformation with the backend

- [ ] Reuse our existing isolated operations to perform a cross-file
  panic-to-typed-error migration and its transitive caller adaptations.
- [ ] Analyze the edited revision, compile the candidate, and run assertions for
  success/failure behavior, source preservation, and boundary policy.
- [ ] Reject a deliberately broken candidate; restore the baseline as another
  revision and confirm that rejected evidence cannot authorize later writes.
- [ ] Cover a recursive caller component and a trait/generic boundary. Resolve
  what upstream can establish and refuse what remains open.

**Gate:** a complete useful migration works with source-backed evidence and
safe rejection. Neither a hover demonstration nor a faster parse is sufficient.

### 3. Complete collection migration and retire previous ownership

- [x] Inventory shared-tree consumer groups and map required backend facts
  above. Continue classifying snippet parsers and validators during each port.
- [ ] Migrate the listed collectors through the existing shared loader.
  Compare normalized IR payloads and findings, with explicit expectations for
  semantic improvements. Run the two backends in separate test executions.
- [ ] Resolve inactive-code and proof-dialect coverage before production cutover.
- [ ] Complete the production parser cutover when its enabled consumers are
  migrated. Remove superseded parse caches, handwritten resolution, and orphaned
  data paths in the same cutover. Do not preserve a permanent dual frontend.

**Gate:** supported lints and apply operations obtain all required facts through
the shared IR, and the old parsing ownership has been removed.

### 4. Measure and resume strategy coverage

- [ ] Record cold load, warm identical query, body edit, signature edit, policy
  change, and rejected candidate recovery with Hotpath/tracing and query
  execution counts where upstream exposes them.
- [ ] Separate project discovery/build scripts/proc macros, syntax, semantic
  analysis, Cordial enrichment, planning, rendering, Cargo/rustc validation,
  and source writes. Report cold cost and peak memory as well as warm savings.
- [ ] Start with specimens, then Cordial and read-only real-library inputs.
  Use the approved Amenable copy for transformations only.
- [ ] Resume nested error-family composition and remaining Owned/Borrowed
  coverage using the new facts. Measure marginal etiquette costs against the
  shared query work; avoid attributing all shared analysis to one lint.

Use focused checks and the build-tier Cordial gate for implementation cycles.
Reserve the all-tier gate for completion of the migration plan.

## Current next step

Port the attribute and file-policy collectors onto the database-issued syntax
carrier and existing IR contracts. Preserve inactive-code policy coverage and
golden findings. Then work through API/layout, tracing, error/apply, and proof
collection in the inventory above. Complete the Cargo context driver before
using semantic facts to authorize transformations. Continue
implement / focused checks / build-tier submit / commit. Keep the selected
architecture as the destination throughout; do not create another user-facing
backend choice or extend handwritten semantic resolution.

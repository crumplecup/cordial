# A shared semantic backend with Salsa and rust-analyzer

**Status:** Architecture exploration. No dependencies or production behavior
changed. Research checked on 2026-10-30; upstream APIs and release versions must
be checked again when implementation begins.

## Purpose

Strategy transformations repeatedly analyze closely related programs. A return
type changes, its callers need adaptation, another error source appears, and
the next candidate needs analysis and compilation. Cordial should concentrate
on constructing the selected design, preserving evidence, and validating the
result. Implementing Rust name resolution, type inference, and invalidation
ourselves consumes effort that could go toward those transformations.

Explore replacing the source frontend with rust-analyzer and extending its
Salsa database with Cordial queries. Keep one canonical analysis backend and
the shared IR boundary. Preserve the existing transformation operations,
evidence requirements, transaction handling, and executable specimens.

This exploration supports the [strategy transformation plan](strategy-transformation.md).
It pauses further composition of nested error migrations while we evaluate the
frontend they should consume. It does not discard completed transformations.

## What exists today

| Responsibility | Current implementation | Consequence for a pivot |
| --- | --- | --- |
| Source ownership and parsing | `src/loader/source.rs`, `source/cache.rs`, `source_work.rs` | One shared `syn::File` per cached path within a parsed-source session; no revision-aware dependency graph across candidate edits. |
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
database for third-party extensions. This makes extending that database the
first integration approach to test. We do not yet have a compiled Cordial
prototype proving that our query types fit its published APIs.

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
| Embedded rust-analyzer with Cordial queries in its database | Shared language analysis and tracked policy facts | Recommended prototype. Requires adapter maintenance and collector migration. |

## Implementation sequence and decision gates

### 1. Prove integration and a meaningful semantic gain

- [ ] Select one published `ra_ap_*` release cohort and verify its Rust and
  Salsa requirements against this repository. The published loader inspected
  during research is `0.0.358`; this is a research reference, not a selected pin.
- [ ] In an isolated test/example harness, load a Cargo workspace once and add a
  Cordial tracked query to its database. Demonstrate invalidation through an
  upstream semantic query. Do not add a parallel production parser.
- [ ] Populate the shared IR for a cross-file panic specimen with an import
  alias, a type alias for `Result`, and a method call. Compare resolved item and
  error identities against fixture expectations, including a shadowed fake
  `Result` and an unresolved call.
- [ ] Change one leaf's return contract. Observe the affected callers, including
  callers of callers. Keep an unrelated module as a recomputation control.

**Gate:** one canonical database, useful semantic facts through `IrView`, and
correct revision invalidation. If published APIs cannot support this boundary,
record the concrete blocker before expanding dependencies or adopting a fork.

Published package reference: [ra_ap_load-cargo](https://docs.rs/crate/ra_ap_load-cargo/0.0.358).

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

### 3. Migrate collection and retire the previous ownership

- [ ] Inventory every `SharedParsedFile` consumer, `syn` visitor, macro-island
  collector, source-span converter, and candidate validator. Map each to a
  backend fact or required adapter capability.
- [ ] Migrate collectors in reviewable batches within the isolated backend.
  Compare normalized IR payloads and findings, with explicit expectations for
  semantic improvements. Run the two backends in separate test executions.
- [ ] Resolve inactive-code and proof-dialect coverage before production cutover.
- [ ] Switch the production source owner only when its enabled consumers are
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

## Recommendation

Proceed to the integration prototype before adding more handwritten semantic
resolution. The expected payoff is broader transformation coverage and less
language machinery to maintain, with incremental reuse across candidates.
Performance gains and the complete migration cost remain unmeasured. Keep
those as prototype results to establish, rather than reasons to skip it.

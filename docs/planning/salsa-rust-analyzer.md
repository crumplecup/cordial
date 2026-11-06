# A shared semantic backend with Salsa and rust-analyzer

**Status:** Active, 2026-11-05. The next milestone is the production
frontend cutover: one rust-analyzer/Salsa database supplies collection and
semantic evidence through the shared IR, and the replaced parsing ownership
is removed. Tracing's remaining semantic work depends on this milestone.
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

## Active milestone: production cutover

The current backend can project semantic evidence in integration tests. The
production pipeline still collects through its compatibility `syn` tree.
Registering text in the database and porting individual collectors are progress
toward adoption; neither establishes that production uses the new frontend.

Complete the following as one milestone, using reviewable implementation
commits and the gates below:

- [ ] Load the production workspace context into the session-owned database,
  including target/features/cfg, dependency roots, build outputs and proc macros.
  Retain the context identity and explicit unavailable evidence in the IR.
- [ ] Migrate every production collector in the inventory below, including
  inactive syntax, macro islands and supported proof dialects. Collect missing
  facts in the loader; consumers read them through `IrView`.
- [ ] Route ordinary quality, submit and apply runs through that database and
  its IR projection. No hook or apply planner creates a private analysis path.
- [ ] Use the projected semantic identities and error formatting capabilities
  in production tracing assessment and apply. Remove the corresponding
  handwritten resolution as its replacement becomes authoritative.
- [ ] Remove the compatibility AST ownership, parse cache and orphaned evidence
  paths at cutover. Classify candidate validation and code generation separately;
  they must not conceal another parse of original production inputs.
- [ ] Demonstrate production tracing across files and one complete strategy
  transformation, including rejected candidates and restored evidence.

Collector ports are implementation steps within this milestone. Do not divert
to independent tracing configuration or recipe refinements while the production
semantic evidence remains unavailable. Do not declare completion on collector
counts or test-only projection results.

### Acceptance evidence

- Normal CLI execution uses the canonical backend with no compatibility
  original-source parse. Exercise both quality collection and apply; adapter
  tests alone do not establish this.
- Compare complete IR payloads and findings in separate compatibility/native
  test sessions before retiring the compatibility implementation. Preserve
  inactive-code coverage and intentional existing findings; document semantic
  corrections with adversarial regressions.
- Compile and execute tracing specimens containing cross-file aliases, borrowed
  success values and trait methods. Verify actual error recording, preservation
  of compliant instrumentation, worklist accounting and a no-op second apply.
  Unsupported or unavailable capabilities must not authorize an edit.
- Check source revision and configuration identity, invalidation after a leaf
  contract change, unrelated query reuse, and rejection/restoration. Candidate
  compilation remains required in addition to semantic analysis.
- Audit source consumers and removed ownership paths, including feature-gated
  collectors, rather than relying only on search counts. Record remaining
  generation/validation parse sites and why they do not duplicate input analysis.
- Use focused checks and build-tier submits throughout. Run the all-tier gate
  at milestone completion, fixing feature-specific compilation issues whenever
  they are encountered during implementation.

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
during integration and found no new items. CI's test job
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

### Attribute projection checkpoint

The loader now collects `SourceFileAttributes` on ordinary source module nodes.
It carries snapshot digests, crate lint declarations, the crate-attribute apply
insertion line, the existing Verus lexical marker, and the supported item
attribute inventory. The database projection produces the same typed contract
from its canonical syntax carrier. Item keys preserve the tracing inventory's
generic impl-method identities; test-only modules keep their existing exclusion.

`AttributeEnricher` materializes `HasAttr` nodes and instrumentation flags from
`IrView`. The crate-attribute scanner and inventory enricher also consume those
facts, including apply insertion positions and Verus exemption evidence. They
no longer walk the shared syntax themselves. The loader resolves custom library
entry paths before collecting facts, including entries outside `src/`.

The parity specimen now includes conditional attributes, doc comments, inner
function attributes, generic trait/inherent methods, inactive modules, and
literal contents that must survive normalization. A graph-only regression runs
attribute enrichment with an empty source view; malformed serialized evidence
is an error. Virtual revisions check lint names versus nested arguments and
literal contents, itemless files, and restored attribute payloads. The expanded
specimen also exposed a receiver-token difference in the namespace projection;
the native collector now preserves that existing contract. Attribute parity
also exposed an old visitor omission: top-level structs and enums did not reach
the attribute attachment routine. Both projections now include these owners.

Focused validation passed: complete source-backend parity, 24 crate-attribute
tests (including the slim crate-attrs feature build), 22 tracing tests, 41
tracing-apply tests, and two attribute-evidence tests.

Dogfooding exposed a tracing apply defect: its pipeline omitted
`AttributeEnricher`, so assessment replaced missing evidence with defaults and
selected compliant functions for destructive rewrites. Apply now runs the
attribute enricher, and assessment rejects missing attribute evidence. A CLI
regression preserves compliant annotations byte for byte, repairs the one
deficient function, checks off exactly one item, and verifies that a second
apply makes no edits. This regression failed before the fix.

The active tracing corrective plan extended the projection with complete
attribute and nested argument ranges. Both backend projections preserve those
facts. A declaration multimap plus file/line joins prevents same-named cfg
alternatives from sharing attributes. Existing annotation repairs now preserve
unrelated choices and use the common source transaction; ambiguous or
unsupported forms remain unchanged. See the active plan for the verified
checkpoint and the remaining observability work.

This completes attribute materialization and crate-attribute policy collection,
not the entire file-policy group. The compatibility collector remains inside
the loader's current single-parse path until production cutover. Bare Verus
declarations outside supported macro islands remain part of the unresolved
proof-dialect migration; the crate-attribute fixture now uses the same `verus!`
input shape as its pipeline counterpart.

### Glob imports and inline-test projection checkpoint

`SourcePolicyInventory` now carries typed, snapshot-bound glob-import and
inline-test evidence as well. Both inventory enrichers consume it through
`IrView`, with explicit errors for missing or corrupt facts. Their shipping
compatibility visitors now belong to loader collection; the enrichers and
standalone crate adapters no longer walk syntax. Native database collectors
produce the same records directly from canonical rust-analyzer syntax.

The complete-payload parity specimen covers grouped and absolute imports,
raw identifiers, prelude exemptions, local modules, inactive syntax, free and
impl test functions, test attributes, cfg-test suppression, and macro payloads.
Graph-only materialization uses an empty source view and compares every
generated policy node's attributes and span. Integration-test and fixture
selection retain their existing scope. Virtual revisions and restoration cover
the complete expanded inventory. Standalone source-string compatibility
adapters remain within loader-owned collection until the frontend cutover.

Validation: the glob-import and inline-test etiquette suites pass, including
their generated-checklist checks. Complete policy parity and virtual
change/restore checks pass with both native collectors enabled. Each feature
also compiles independently without warnings. The build-tier commit gate
passes with zero open items; the all-tier gate stays at the migration completion
milestone.

This checkpoint retired syntax access in these two enrichers. Modularity and
pageantry completed the remaining file-policy ports in the checkpoint below;
production input ownership still awaits the full collector migration.

### Modularity and pageantry projection checkpoint

`SourcePolicyInventory` now also records file, module, type-count and function
size facts, item-order findings, and proc-macro shim sizes. Collection preserves
small bodies and compliant shims: current thresholds, generated-file exclusions
and rule switches are applied when materializing policy nodes from `IrView`.
Neither enricher reads syntax or source text. Their compatibility visitors now
belong to loader collection and use its existing shared parse; native collectors
read the canonical rust-analyzer tree directly.

The parity specimen covers documented and inactive modules, nested functions,
impls and trait defaults, trait aliases, closure and test exclusions, anonymous
constants, barrel declarations and proc-macro shims. It compares complete typed
records and every materialized node's attributes and span with an empty source
view. Changing thresholds and rule switches against the same graph verifies
that collection does not discard facts needed by later policy decisions.
Virtual edits and restoration exercise module identity and shim body sizes.
Modularity retains source fixtures and excludes integration tests; pageantry
excludes fixtures and includes integration-test files, matching existing scope.

Validation: all 48 modularity tests and 19 pageantry tests pass. Backend parity,
policy changes, virtual edits and restoration pass with both collectors enabled;
each etiquette also compiles independently without warnings. The build-tier
commit gate passes with zero open items; the all-tier gate remains reserved for
migration completion.

The file-policy group now has native collectors and graph-only consumers.
Production cutover still depends on the API/layout, tracing, error/apply and
proof collectors, together with complete Cargo context ownership.

### Derive evidence projection checkpoint

`SourcePolicyInventory` now carries typed derive declarations: struct fields,
visibility and wrapper shapes, generated builder names, inline module gates,
trait membership, receiver properties and supported method body shapes.
Collection preserves constructor arities and fluent setters independently of
configured thresholds. Derive policy consumes these facts through `IrView`;
its enricher no longer walks source syntax. The compatibility helpers moved
into loader collection and still use the shared parse. The native collector
reads the canonical rust-analyzer tree, including inactive syntax.

Parity coverage compares complete evidence payloads and all materialized
finding attributes and spans with an empty source view. It exercises all eight
derive rules, generated builders, Copy and borrowed getters, Box and Option
exemptions, setter conversions, constructor forwarding, const and error
constructors, test and Creusot gates, tuple fields, source fixtures and
integration-test exclusions. Threshold changes reuse the same graph; virtual
body edits invalidate the affected facts and restoration recovers the original
payload and findings.

All 39 existing derive tests and the dedicated backend parity test pass with
only the derives feature enabled. The collector and policy also retain their
existing full-feature test coverage.

The shared workspace path-inclusion inventory supplies consumer dependency
checks. Its collector migration is described below. Visibility and CLI layout
remain before the API/layout group is complete. Production source ownership
has not switched backends yet.

### Workspace module-splice projection checkpoint

`PathInclusionFacts` now belongs to the shared IR. It retains typed Cargo member
and dependency inputs, each selected source file's digest, and ordered module
`#[path]` targets, alongside deterministic indexes used by derives and tracing.
Collection belongs to the loader: the compatibility path uses shared source
carriers; the native database projects the same source selection from its
canonical trees. Consumer modules no longer discover or traverse source syntax
for these facts.

The native projection accepts the captured workspace selection from IR. It
reuses member/dependency context and recomputes source evidence at the current
database revision. It does not run another Cargo query or filesystem source
scan. An unavailable source or invalid tree fails the projection. Shipping
collection now propagates metadata and source errors instead of reporting an
empty graph; manifest-free projects retain explicit synthetic-target support.
The standalone `workspace_path_inclusions` API consequently returns a result.

Parity coverage compares complete typed evidence and lookup indexes across
backends. It covers inactive and nested modules, repeated attributes, raw string
paths, source fixtures, integration-test exclusions, macro/function-local
exclusions, renamed dependency edges, transitive dependents and derive blockers.
Virtual changes remove and restore splices and their downstream recommendations;
invalid syntax and newly discovered unavailable inputs fail closed.

Validation: all 39 derive tests and both workspace evidence tests pass with
only the derives feature enabled. The workspace evidence tests and all 46
tracing apply regressions also pass with full features enabled.

Path target resolution and declared dependency policy retain their existing
semantics. The member selection and dependency inputs still originate in the
shipping Cargo discovery path. Moving that context's ownership to the native
session remains part of the complete Cargo context driver; this checkpoint
does not switch production source ownership.

### Visibility module-tree projection checkpoint

`SourcePolicyInventory` now carries a typed visibility module tree, including
declared visibility, ancestor relationships, direct name counts, declaration
locations and the snapshots of every contributing file. Both collectors feed
one loader-owned tree assembler. The compatibility collector uses shared source
carriers; the native collector reads canonical database trees, including inactive
syntax. Visibility policy and its enricher consume only shared IR, with no source
walk or filesystem module discovery downstream.

Parity coverage compares the complete tree, snapshots and all finding attributes
and spans. It includes inline and external modules, nested module files, explicit
paths outside `src`, re-export groups and aliases, restricted visibility, hidden
and test modules, and macro/function-local exclusions. An empty source view still
materializes the same findings. Threshold changes reuse captured evidence;
virtual external-file edits change it and restoration recovers it. Missing or
malformed inventory and invalid native source fail closed. Cyclic module paths
fail rather than recursing without bound.

Validation: all 13 full-feature visibility tests pass, as do the 11 tests with
only visibility enabled. The two dedicated visibility backend regressions pass
in both configurations; the existing source backend parity suite also passes
with full features and with visibility alone. The build-tier commit gate reports
zero open items. The all-tier gate remains reserved for migration completion.

Root selection and module path resolution retain the existing policy. Moving
their ownership to complete Cargo target context remains part of the context
driver. Native collection explicitly rejects `verus!` visibility bodies when
the dialect feature is enabled; it cannot silently replace their declarations
with zero counts. Shipping collection retains its existing Verus support until
the dialect capability is migrated. Production source ownership has not switched
backends. The following checkpoint completes the CLI layout collection group.

### CLI layout collection checkpoint

`SourcePolicyInventory` now carries a required, typed CLI catalog: library and
binary ownership, clap/error classification, field and variant relationships,
resolved `act` targets, free-function argument types, entry-point shapes and
source fingerprints. The enricher and policy evaluator consume these facts
through `IrView`; neither discovers modules nor walks source syntax.

The shipping collector now lives at the loader boundary. The native collector
uses canonical database syntax directly, including inactive declarations,
external and inline modules, explicit module paths, and separate binary entries.
It resolves the existing nested-dispatch cases over native expressions and
patterns: direct/associated calls, field forwarding, destructuring, transparent
`Some`/`Ok` wrappers, local bindings, match arms, `if let` and `while let` scopes.
No native-to-syn fallback is introduced. Pending syntax is transient collection
state; only resolved evidence is serialized into shared IR.

The dedicated backend regression compares complete catalogs and findings,
materializes policy with an empty source view, checks required evidence and
test/fixture/function-local module exclusions, and exercises virtual edits,
exact restoration, and invalid binary syntax. Existing
root selection, short-name resolution and filesystem-based module discovery
remain policy-compatible; Cargo context and semantic identity work are still
required before production cutover.

Validation passed with full features and with CLI layout alone: the new catalog
and finding parity regression, all 14 existing CLI-layout tests, and the shared
source-backend regressions. The latter also retain their malformed-evidence
rejection checks. The build-tier commit gate reports zero open items; the
all-tier gate remains reserved for migration completion.

### Collector migration inventory

The starting audit found 53 files consuming `.syntax()` or `.macro_syntax()`
from shared carriers (75 call sites before the new adapter). Port collection
in the following dependency order. Preserve existing fact payloads wherever
possible so consumer stages do not need a simultaneous rewrite.

| Collection group | Current entry points | Required backend capability / retirement |
| --- | --- | --- |
| Source ownership and graph population | `loader/source.rs`, `source/cache.rs`, `source_work.rs`, `populate.rs` | Canonical text inputs first; then syntax carriers, source ranges, item/module population, inactive syntax. Remove `ParsedSource.syntax`, the thread-local AST cache, and the `syn` parser on cutover. |
| Syntax/source facts and direct calls | `ir/function_source.rs`, `ir/source_calls/collect.rs`, `enricher/source_calls.rs` | Signatures, exact edit ranges, declaration identity, instantiated contracts, resolved calls. Replace name-based resolution using semantic facts. |
| Attributes and file policy | Attribute materialization, `crate_attrs`, `allows`, `cfg_scatter`, `cfg_hygiene`, `glob_imports`, `inline_tests`, `modularity` and `pageantry` consume loader-projected IR | Unexpanded syntax including inactive items, token/range fidelity, nested attributes. Keep normalization and golden findings tests. |
| API and layout | Derive, visibility and CLI layout policy consume loader-projected typed IR with native collectors. Module-splice collection also has shared IR evidence and native parity. | Item signatures, visibility, derives, `#[path]`, inline/out-of-line modules and Cargo roots. |
| Tracing | Function inventory, subscriber/boundary/stdio policy and workspace proof reachability consume loader-projected typed IR with native collectors. | Preserve inactive syntax, macro inputs, executable boundaries, apply ranges and worklist accounting. Replace the existing name-based reachability policy with semantic evidence after Cargo context is complete. |
| Error analysis | Sites, chain records, type relationships, expression compliance, signature-chain and architecture assessment consume loader projections with native collectors (`SourceErrorSites`, `SourceErrorChains`, `SourceErrorTypes`, `SourceErrorCompliance`, `SourceErrorSignatures`, `SourceErrorArchitecture`). Error apply now also has native loader projections as described below. | Error/source identity, signatures, conversions, return contracts, owned/borrowed storage and source-chain evidence. |
| Error apply inventory | Both loaders project required `SourceErrorApply` declaration, namespace, name/initializer safety, located graph-use, graph-expression, graph-lifetime, getter storage, getter consumer, local lifetime, constructor-initializer and conversion-site contracts. Conversion-site selection consumes required IR records; the original batch syntax visitor is removed. | Declaration payloads include storage, source/Kind edges, generic and lifetime shapes, visibility, attributes and edit ranges. Module boundaries, import bindings and type-alias names drive graph and getter identity joins from IR. Name-shadowing guards, immutable string declarations, graph-closure uses, constructor values, field accesses, macro guards and expression lifetime origins also come from loader evidence. Graph lifetime evidence includes declaration and signature ranges, storage requirements, constructor anchors and safe forwarding returns. Getter storage contracts retain alias ownership, collision counts, conditional Display guards and exact insertion/removal edits. Getter consumer recipes preserve lexical receiver scopes, method contracts and exact macro payload origins; the crate-wide identity join consumes IR alone. Local lifetime contracts retain constructor, forwarding, allowed type-use and exact edit-range evidence. Constructor-initializer recipes retain immutable binding provenance, caller aliases, cfg-paired tuple origins, impl lifetime requirements and exact expression ranges. Conversion-site recipes preserve ordered uses, return scopes, From/source/try evidence, generated-constructor guards and macro field uses. Keep edit rendering text-only. |
| Panic and proof collectors | Both loaders project required `SourceVerus`, `SourcePanics` and `SourceAntipatterns` graph evidence. Proof-pattern, panic and antipattern hooks consume IR alone. Native collection covers ordinary syntax, Kani proof graphs, embedded programs and Creusot/Kani/Verus contract clauses. | Canonical outer Rust plus memoized payload grammars. Retire compatibility collection with the production frontend; no fallback whole-file parser. |
| Candidate validation and generation | source-write/transform validators, test/compliance scan helpers | Separate proposed-output validation and token generation from original-source collection. Port validators deliberately; do not mistake fixture-only parsing for production input ownership. |

Macro islands and snippet parsers need their own inventory in addition to shared
carrier consumers. The audit found 36 files mentioning `syn`/`verus_syn` parse
entry points; some are documentation, fixture helpers, generation, or proposed
edit validation. Classify each before removal. A text search count is not proof
that every production path has been migrated.

Complete production integration of the input/context driver before semantic
facts authorize transforms: selected target/features/cfg, build-script outputs,
proc macros, manifest and lockfile identity. Native sessions now load build
outputs and retain the proc-macro service, as recorded below. Missing expansion
and unsupported call/type evidence must stay incomplete.

The file policies now share a typed, snapshot-bound
`SourcePolicyInventory` on the crate root. Enrichers consume this inventory
through `IrView` and reject missing or malformed evidence. The loader owns the
temporary shared-syn collectors; the native database projects the same facts
from its canonical syntax, including inactive items. Declaration inputs for
cfg hygiene (`Cargo.toml`, inherited workspace lints, and `build.rs` text) are
captured in the source database and fingerprinted with the source files.
Evaluating build scripts remains a context-driver obligation. These ports
prepare the production cutover; normal execution still uses the shared-syn
frontend until its remaining consumers migrate.

### 2. Complete production context and collection migration

- [ ] Complete the session-owned Cargo context driver described above. Bind
  projections to the selected context, and invalidate affected facts when its
  inputs change. Missing context or expansion support remains an explicit gap.
- [x] Inventory shared-tree consumer groups and map required backend facts
  above. Continue classifying snippet parsers and validators during each port.
- [ ] Migrate the listed collectors through the existing shared loader.
  Compare normalized IR payloads and findings, with explicit expectations for
  semantic improvements. Run the two backends in separate test executions.
- [ ] Resolve inactive-code and proof-dialect coverage before production cutover.
- [ ] Complete the production parser cutover when its enabled consumers are
  migrated. Remove superseded parse caches, handwritten resolution, and orphaned
  data paths in the same cutover. Do not preserve a permanent dual frontend.
- [ ] Wire production tracing to the shared semantic contracts and formatting
  capabilities; verify assessment, apply, runtime events and worklist outcomes
  against cross-file specimens.

**Gate:** supported lints and apply operations obtain all required facts through
the shared IR in normal CLI execution, production tracing uses semantic facts,
and the old parsing ownership has been removed.

### 3. Exercise one complete transformation with the backend

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

### Tracing observability collection checkpoint

Subscriber initialization, binary error boundaries and stdio policy now consume
required `SourceObservability` evidence through `IrView`. Their enrichers no
longer traverse source syntax. The loader projects one configuration-independent
inventory of file snapshots, function fallibility, error instrumentation, body
calls and stdio sites. Policy evaluates helper paths, rule switches and skip paths
against that inventory. Missing or malformed fields fail closed.

The shared-syn compatibility collector and native database collector project the
same complete payload, including inactive code, nested functions and modules,
trait default bodies, opaque macro boundaries and literal decoding. Deliberately
invalid fixture files carry an explicit invalid-syntax state: default fixture
exclusions still work, and including those files cannot establish compliance.
Native collection uses canonical database trees without a parser fallback.

Regression coverage compares complete inventory and finding payloads, runs the
three enrichers with an empty source carrier, changes policy without recollection,
rejects missing/corrupt evidence, and tests virtual edits, restoration and invalid
executable source. Existing subscriber, boundary and stdio tests remain the
behavioral reference. This checkpoint does not complete the tracing cutover:
workspace proof call-graph collection and its downstream reachability evidence
still need native projection before retiring the shared-syn carrier.

## Checkpoint: workspace proof call evidence

The workspace source inventory now retains required `SourceProofFunction`
evidence beside each source snapshot: qualified identities, call keys, inherited
cfg and verifier markers, and direct path-call keys. Compatibility collection
runs during the existing workspace inventory walk. Native collection consumes
canonical database trees. Tracing reachability reads only this shared IR;
it no longer discovers files, clones function bodies or visits source syntax.
Policy can change without recollecting source evidence.

Macro token trees remain opaque in the file grammar. Supported assertion
arguments and harness item blocks are projected at the loader boundary. Native
macro grammar projections use memoized Salsa queries; compatibility collection
uses the existing snapshot-owned macro cache. No lint parses these islands.
Native collectors can only look up syntax issued from the canonical snapshot;
foreign macro trees cannot populate the loader's projection table.
Recognizing Rust items inside a harness does not claim knowledge of its expansion.

Regression coverage compares the complete workspace evidence and expected
cross-crate exclusion sets, including runtime callers, inactive items, generic
and trait impl identities, nested harnesses and assertion message boundaries.
Virtual edits replace calls without changing function identities, change runtime
reachability, and restore the original payload. Missing or malformed evidence
fails; a serialized inventory supports policy re-evaluation after deleting source
files and dropping the database.

This is a collection migration. The existing name-based resolution and fixed
point remain unchanged; unresolved receiver calls and external callers are not
proof of absence. Semantic proof reachability still depends on the complete
Cargo context driver. Normal execution retains the shared-syn frontend until
the remaining consumer groups migrate together.

## Checkpoint: error-site census

The loader retains a required `SourceErrorSites` inventory on each crate root.
It includes snapshots of every scanned file, including files with no sites,
and the complete records for propagation, mapping, explicit returns, conditional
and match handling, and option conversion. Both collectors issue this same typed
payload. The native collector walks canonical database syntax directly; it does
not parse another representation or consult a compatibility tree.

The site enricher consumes only `IrView`, materializing the existing expression
nodes and attributes. The remaining compatibility chain collector joins its
annotations to those nodes by location instead of collecting the site census
again. Standalone site fixtures also obtain their trees through the shared
loader. This retires site collection from the downstream unified error scan;
chain, architecture, type and apply evidence still need migration.

Regression coverage compares complete records, source digests, file coverage
and materialized attributes and spans across both backends. It covers all six
site kinds, inactive code, exact test-module exclusions, nested functions and
modules, trait and impl contexts, opaque macros, Unicode truncation and fixture
exclusions. Virtual edits replace evidence and exact restoration recovers it.
Missing or malformed inventory and invalid native syntax fail closed. Graph-only
materialization still works after dropping the database and deleting source files.

Validation: 71 focused tests passed with full features, covering site payload
parity, site/chain sessions, flow, plugin registration, internal compliance and
foreign-error consumers. The site-only configuration passed its nine focused
tests and a warning-free check. After grouping the chain-only implementation,
the 20 site/chain/flow/plugin tests and eight site-only tests passed again.
The chain-only configuration also checked without warnings. Build-tier submit
completed with zero open items; the deep tier remains for the plan milestone.

Production source ownership remains with the compatibility frontend until the
remaining collection groups and Cargo context are ready for the complete cutover.

## Checkpoint: loader-owned error-chain contract

Error-chain records now enter a required, snapshot-bound `SourceErrorChains`
inventory during loading. The graph-only chain enricher joins those records to
located site nodes; it neither reads source nor invokes the compatibility scan.
The remaining source hook is compiled only for internal architecture/compliance
and apply. Its unified walk no longer repeats chain collection. Standalone chain
fixtures now obtain their trees through the shared loader as well.

A regression compares standalone and loader records for all five chain rules,
checks missing/corrupt evidence, and replays the production site/chain hooks after
deleting original source files. The native collector is the immediate next step;
this contract checkpoint does not switch production source ownership.

Validation: the chain, flow, plugin and internal-error suites passed with full
features (54 tests across the focused runs). The chain-only replay test passed
without warnings after tightening the internal helper gate. Build-tier submit
finished with zero open items; deep remains reserved for the plan milestone.

## Checkpoint: native error-chain projection

The canonical database now projects `SourceErrorChains` directly from its tracked
rust-analyzer trees. All five chain rules share the loader contract and ordinary
graph materialization; the native path does not invoke the compatibility parser
or reconstruct evidence downstream.

The parity fixture compares complete records, snapshots, empty-file coverage,
materialized attributes and spans. It covers inactive code, nested contexts,
qualified types, shorthand fields, preserving and stringifying converters,
opaque macros and Unicode truncation. Virtual edits change and restore the
inventory without modifying disk; malformed inventory-only inputs fail closed.
The graph hooks also replay after source deletion. Production source ownership
still changes only with the complete collection and Cargo context cutover.

Validation: complete native chain parity and virtual-revision tests passed with
full features, alongside the chain and flow suites. Native site parity also
passed after extracting their shared syntax helpers. The chain-only build was
warning-free. Build-tier submit finished with zero open items.

## Checkpoint: loader-owned and native error type relationships

The shared loader now retains required `SourceErrorTypes` evidence: per-file
snapshots, raw type relationships and error implementors, including empty files.
Crate-wide filtering, graph metrics and expression materialization consume that
inventory through `IrView`. The compatibility error scan no longer collects or
materializes this type graph. Standalone fixtures use the shared parse carrier.

The native collector reads the canonical database tree directly. It preserves
the existing policy grammar, module names, source targets, spans and snippets;
this port does not claim semantic resolution of those names. Complete raw
payload and graph parity covers struct, enum and source-method relationships,
derive and cross-file implementors, inactive code, nested modules, qualified
types, opaque macros, test-module exclusions and Unicode. Virtual changes to
an implementor invalidate the dependent graph without touching disk. Restoration
recovers the original evidence, malformed inputs fail closed, and production
materialization replays after source deletion and database disposal.

Production source ownership still awaits the remaining error/apply and
panic/proof collectors and the complete Cargo context driver. The compatibility
frontend is retired at that cutover, not retained as an alternative backend.

Validation: 51 focused tests passed with full features across the type parity,
chain, flow and internal-error suites. Both type-inventory tests also passed
with only `internal_error_chain` enabled, without compiler warnings.
After consolidating the collector layout and feature registration, the full-feature
type parity suite passed again. Build-tier submit verified all three organization
fixes and finished with zero open items. The deep tier remains reserved for the
end-of-plan milestone.

## Checkpoint: loader-owned and native expression compliance

Required `SourceErrorCompliance` evidence now retains per-file snapshots and
complete expression findings, including files with no findings. The compliance
materializer reads this inventory through `IrView`; downstream error enrichment
no longer walks source for stringification and typed-error discard records.
Standalone fixtures use the shared loader carrier instead of parsing privately.

The native collector consumes canonical rust-analyzer expressions and preserves
the existing policy grammar. Complete payload and materialized graph comparisons
cover caught bindings, block results, source retention (including shorthand
fields), opaque macro tokens, comments and whitespace, nested contexts, inactive
syntax, Unicode and the proc-macro diagnostic terminus. Virtual edits invalidate
and restore the inventory without disk writes. Malformed inputs and missing
required evidence fail closed. Materialization replays after source deletion
and database disposal.

Validation: the 49 existing internal-error, chain and flow tests passed, followed
by both full-feature compliance contract tests. Both contract tests also passed
with only `internal_error_chain` enabled, without compiler warnings. Gate cleanup
uses the tracing recipe and centralizes typed evidence decoding separately from
the public IR view trait and concrete graph storage. The build-tier submit
passed with zero open items.
Production still uses compatibility collection until the remaining collectors
and Cargo context driver are ready for the complete ownership cutover.

## Checkpoint: signature-chain evidence joins the shared graph

`SourceErrorSignatures` retains file snapshots and the complete declaration,
import, source-relationship and conversion inventory. Parser nodes are lowered
to a closed typed grammar before entering shared IR. The signature materializer
assesses those graph facts through `IrView`, without reading source or keeping
an auxiliary index. Required inventory fields fail closed; unsupported shapes
remain unknown. The existing resolution grammar is preserved rather than
expanded during migration.

Contract coverage compares all eight existing conversion outcomes and complete
serialized facts, checks source-free materialization, and mutates a retained
source contract to prove assessment consumes that evidence. The native collector
now obtains the same inventory from canonical database trees. Complete payload
comparisons cover generic declarations, imports, source implementations, inactive
code, nested functions, qualified paths and opaque macro boundaries. Virtual
source-contract changes invalidate assessments, restoration recovers all facts,
and malformed input fails closed. Cargo context integration still gates
production semantic authority.

Validation: the native parity fixture and all 39 internal-error tests passed
with full features. Native parity and required-inventory replay also passed
with only `internal_error_chain` enabled. The build-tier submit finished with
zero open items after clearing encapsulation, modularity and cfg grouping
findings. The deep tier was not rerun.

## Checkpoint: architecture policy consumes retained contracts

Required `SourceErrorArchitecture` evidence retains the complete architecture
catalog and source snapshots. Field storage is lowered to a closed contract
grammar, including Copy requirements, static-string aliases and borrowed String
containment. Parent/Kind/source relationships, constructors, caller-location
attributes, tracked `map_err` sites and literal fan-out sites share that inventory.
The graph-only enricher evaluates strategy and fan-out policy through `IrView`;
no syntax or auxiliary catalog survives into assessment.

Collection preserves library module reachability, excluding binary-only and
orphan files when a library root exists. Macro argument syntax for fan-out now
comes from the shared loader cache rather than a local payload parser.
Regression coverage reassesses Owned/Borrowed and fan-out policy without
recollection, checks complete serialization and required fields, mutates retained
Copy evidence, and replays materialization after source deletion.

The native architecture collector now produces the same contracts directly from
canonical database trees. Library reachability follows virtual module revisions,
including inline modules and explicit paths. Field aliases, generic shadowing,
Copy requirements, qualified conversions, constructor attributes and opaque macro
boundaries have complete payload parity coverage. Attribute values and literal
fan-out macros use the loader's memoized grammar queries; collectors do not parse
payloads privately. Assessment and materialization replay after source deletion
and database disposal. Production retains the compatibility frontend until the
remaining consumers and Cargo context support the cutover.

Validation: the required-inventory regression and all 39 internal-error tests
passed with only `internal_error_chain` enabled. The inventory regression also
passed with full features. Tracing apply checked off both gate findings, and
build-tier submit verified them with zero open items. The deep tier was not run.

Native validation on 2026-11-05: complete architecture parity passed with the
isolated etiquette and with full features, including generic and closure commas
in attribute values. The full-feature CLI-layout, error-chain, error-site,
signature-chain and internal-error suites passed with the shared grammar and
module resolver changes. Tracing apply repaired two annotations; build-tier
submit verified zero open items (list 45). The deep tier was not run.

### Cargo context driver checkpoint

Native `SourceDatabase::load_with` now uses one explicit Cargo configuration for
metadata, build outputs and database loading. It supports feature selection,
target, all-target collection, test cfg and environment overrides. Build scripts
run outside tracked queries, their outputs enter the same database, and the
session retains the proc-macro client for subsequent semantic queries.

Each `SemanticSnapshot` carries typed workspace evidence: effective crate cfg,
edition and dependency roots; toolchain, sysroot and macro-service paths;
configuration presence and digests; environment digests without secret values;
the initial database-source digest; and explicit loading gaps. Identity covers
that complete frozen context. Virtual source revisions remain separate.

Manifest, lockfile, discovered Cargo/toolchain configuration and build-script
entry bytes are checked before projection. Newly created configuration files
and changed ambient environment require reload. Virtual edits cannot pretend to
rebuild a Cargo input or a proc-macro crate. Reload builds a replacement first;
failure preserves the previous database and revision.

The workspace specimen exercises build-generated aliases, cfg, local dependency
calls, actual proc-macro expansion, feature/environment reloads, source-only
virtual changes, complete snapshot serialization and failed reload restoration.
A failed build leaves explicit context gaps and unavailable error-formatting
capabilities rather than proving a non-fallible contract.

Remaining context work before production cutover: connect production target and
configuration selection to this driver; track arbitrary non-Rust build-script
and macro inputs for automatic reload; establish target-context
selection where a file belongs to multiple crate instances. The driver freezes
one loaded context; it is not yet a filesystem watcher or the production router.

Validation on 2026-11-05: the Cargo-context specimen passed with full features;
the failure-path specimen and existing semantic revision/reuse test also passed
without default features. Tracing apply repaired one annotation, and the
build-tier submit verified all nine claims with zero open items (list 47).
The deep tier was not run.

Compiled-input invalidation now follows the HIR crate dependency closure rooted
at build scripts and proc macros. Module definitions, explicit out-of-directory
paths and `include!` inputs are retained with exact source digests in the shared
workspace context. Virtual edits to those inputs require reload; disk changes
are rejected before projection. This replaces the proc-macro folder heuristic.
Toolchain library sources do not build these artifacts and are excluded from
this compiled-source inventory. Ordinary application source remains editable.

Validation: both Cargo-context tests passed with full features, including the
`include!` input and compiled dependency guards; the earlier dependency/module
specimen also passed without default features. Build-tier submit finished with
zero open items (list 48). The deep tier was not run.

### Verus dialect ownership checkpoint

The loader now projects `SourceVerus` onto the crate graph root. Each scanned
file carries its source fingerprint and serialized function, panic and enum
facts, including empty files. `IrView` requires this evidence and rejects
missing or malformed payloads. The old workspace Verus map, preparatory scan
and cache accessors are removed. Panic and verifier-warning enrichers read the
graph inventory; proof-pattern enrichment no longer requires a source view.

The native path selects macro bodies from canonical database syntax. A Salsa
query parses each Verus payload with its required dialect grammar and retains
context-free facts plus visibility counts. The loader binds them to the issuing
snapshot, module and test context; native visibility uses the same query.
The dialect grammar does not parse the outer Rust file. Invalid outer syntax
fails projection. Existing best-effort handling of malformed dialect bodies
and exclusion of qualified macro paths are preserved.

Canonical syntax carriers and macro grammar adapters now live together under
`loader/native_syntax`; Cargo sessions, semantic queries and graph orchestration
remain under `loader/database`. Normal execution still uses the compatibility
frontend. Its dialect adapters remain until that frontend is retired; this
checkpoint does not claim that production cutover is complete.

Validation on 2026-11-05: complete serialized Verus and visibility payloads
match the compatibility frontend. Virtual edits, restoration, malformed outer
syntax, missing evidence and source removal are covered. Query metrics confirm
that repeated projection reuses the dialect parse. The parity and Verus tests
passed with full features and with only `verus_ir`; panic and proof-pattern
tests passed with full features. Build-tier submit cleared all structural
findings and finished with zero open items (list 52). The deep tier was not run.

### Panic graph ownership checkpoint

The compatibility loader now retains `SourcePanics` on the crate root, including
every scanned source fingerprint, panic record and the Kani proof call graph.
The panic enricher reads this required typed evidence through `IrView`, without
source access. Missing or malformed inventory fails rather than silently
reconstructing it. The stored proof graph preserves the existing bare-name
exemption policy; this is not a claim of semantic call resolution.

Standalone panic scans now receive their outer syntax through the shared source
carrier too. Native ordinary-panic collection and replacement of compatibility
macro and embedded-program parsing remain cutover work.

Validation on 2026-11-05: full-feature and isolated `panics` tests passed.
Regression coverage compares complete serialized inventory and materialized
node attributes and spans after source deletion, including cross-file proof
exemptions, cycles, `cfg(not(kani))`, embedded programs, error assertions and
ignored fixture/parity trees.
Build-tier submit verified zero open items (list 54). The deep tier was not run.

### Native panic collection checkpoint

The source database now projects the complete panic inventory, including source
fingerprints, ordinary panic sites, embedded programs, error-assertion exemptions
and the existing bare-name Kani proof graph. The loader selects opaque macro
payloads and embedded program literals from canonical syntax; Salsa owns and
memoizes their required grammar queries. Embedded collectors receive a
loader-issued source/tree carrier. The optional Verus grammar and the fallback
function-body recovery preserve their respective compatibility policies.

Full-feature and isolated `panics` regressions compare the entire serialized
payload across all 28 panic specimens, cross-file proof graphs, virtual edits,
restoration and source deletion. Query metrics verify reuse on unchanged
projection. Invalid outer syntax fails projection. Compatibility production
ownership remains until the remaining collector and Cargo-context work is done.
Build-tier submit verified zero open items (list 57). The deep tier was not run.

## Checkpoint: antipattern policy consumes retained source facts

Antipattern collection now retains a required `SourceAntipatterns` inventory on
the crate graph. It includes inspected-file fingerprints, ordinary candidates,
static-field type shapes and error-type membership, and proof-clause call shapes,
canonical bodies and sibling relationships. The antipattern enricher is an
`IrEnricher`: it evaluates current policy and workspace registry evidence without
source access or reconstructing source syntax. Registry fragments remain external
workspace inputs; they are not substitutes for source evidence.

Borrowed error policy, static-reference guidance, named contract recognition and
workspace version findings retain their existing behavior. Standalone source
entry points obtain loader-issued shared trees. Native projection of these facts
is still required before retiring the compatibility frontend.

Validation on 2026-11-05: isolated `antipatterns` coverage passes all existing
ordinary and contract-shape cases. New regressions round-trip the complete
inventory, reject absent or malformed evidence, and evaluate all six policy
combinations and changed registries for Kani, Creusot and Verus after source
removal. Full-feature checks pass, and build-tier submit verified zero open items
(list 59). The deep tier was not run.

## Checkpoint: native antipattern collection

The database now projects the complete `SourceAntipatterns` inventory from its
canonical syntax carriers: ordinary candidates, field storage shapes and error
membership, inspected-file fingerprints, and Kani, Creusot and Verus clauses.
The existing IR-only assessment consumes the same payload from either loader.
Inactive declarations, cfg siblings, foreign signatures, fixture exclusions and
proof-specific scan roots retain their existing policy coverage.

Opaque contract dialects use loader-owned, tracked grammar queries. Kani call
shapes come from canonical Rust expressions. Proof harnesses share the memoized
embedded-program grammar with the other native collectors; their source carriers
bind the original file coordinates to the exact issued body. Nested item
harnesses preserve their context and locations. Statement harnesses remain
opaque, as required by the existing contract policy.

The parity regression compares the complete serialized inventory against all
ordinary specimens and all three proof dialects. It checks virtual edits,
restoration, invalid outer syntax, retained evidence after source deletion and
database disposal, and grammar-query reuse on unchanged projection. Production
ownership still awaits the remaining error/apply collectors and Cargo-context
integration; this checkpoint does not switch the production frontend.

Validation on 2026-11-05: all 44 isolated antipattern tests pass, including
complete native/compatibility payload parity and the ordinary, graph-evidence
and contract-shape suites. Full-feature native parity passes for antipatterns,
panics, tracing proof evidence and error architecture. Panic-only parity also
passes, with a warning-free isolated compilation. Build-tier submit cleared all
findings and verified zero open items (list 62). The deep tier was not run.

## Checkpoint: error/apply declaration projection

Both loaders now project a required, snapshot-bound `SourceErrorApply`
inventory. It records every reachable module's complete struct and enum
declarations: fields, variants, storage shapes, source and Kind edges,
visibility and attributes, generic and lifetime restrictions, and original edit
ranges. Unsupported field types remain in the inventory so graph safety checks
can reject incomplete conversions.

The production apply index consumes these declarations from shared IR and joins
the existing getter evidence afterward. It rejects missing, malformed,
ambiguous or stale declaration evidence instead of reconstructing it from the
source. The native collector uses canonical database syntax without a second
parser. Remaining expression, use, initializer, getter and lifetime collectors
still use the shared compatibility tree; this checkpoint does not retire that
frontend or switch production ownership.

Validation on 2026-11-05: complete declaration parity and production-consumer
tests pass with full features and with `internal_error_chain` alone. Existing
apply, cross-file and getter round-trip suites pass with full features. Parity
also exposed a compatibility edit-range bug: proc-macro columns count
characters while edits use bytes. The conversion now handles Unicode prefixes,
with an exact-range regression assertion. Missing, malformed, duplicate and
stale declaration inventories fail closed.
Build-tier submit verified zero open items (list 66). The deep tier was not run.

## Checkpoint: error/apply namespace projection

Both loaders now collect module boundaries, import bindings and type-alias names
into the required `SourceErrorApply` inventory. Native collection uses the
canonical database tree. The crate graph index and getter receiver identity
index consume this evidence, together with declaration identities and field
names from the previous checkpoint. They no longer rescan source syntax for
these joins. Snapshot validation applies to the whole per-file payload.

The projection retains grouped and renamed imports, underscore bindings,
conditional import blockers and conditional module boundaries. Declaration
link eligibility preserves the existing distinction between compiler derives
and attributes that make an identity unsafe to link. Missing namespace evidence
fails closed rather than triggering a compatibility-tree reconstruction.

Getter method/body analysis, expression safety, initializers and lifetime
collection still use the shared compatibility tree. These are the next native
collector groups; the namespace port does not complete production cutover.

Validation on 2026-11-05: complete frontend parity and production-consumer
tests pass with full features and with `internal_error_chain` alone. The full
cross-file and getter round-trip suites pass. Consumer mutation tests verify
that module boundaries govern executable graph edits and imports and aliases
govern getter identities. Build-tier submit verified zero open items (list 68).
The deep tier was not run.

## Checkpoint: error/apply name and initializer safety

Both loaders retain name-shadowing guards and immutable file-level string
declarations in the required, snapshot-bound `SourceErrorApply` inventory.
Production apply consumes those records when building its crate index and
conversion units. It no longer reconstructs these guards from source syntax.
The native collector reads the canonical database tree.

Name guards preserve protected declarations, generic type parameters, constructor
aliases, grouped imports, wildcard blockers and the boundary around inline
modules. Free declarations remain distinct from associated and foreign items.
String initializer sources require one unconditional, immutable declaration
with the supported static string reference type; duplicate, conditional,
mutable or documented declarations do not establish that evidence.

Remaining located-use, expression, getter, initializer-expression and lifetime
collectors still use the shared compatibility tree. Production cutover remains
pending those ports and the Cargo-context obligations above.

Validation on 2026-11-05: complete safety parity and production-consumer
regressions pass with full features and with `internal_error_chain` alone.
All 47 existing error-apply tests pass with full features. Consumer mutation
tests verify that name guards change graph plans and immutable-source evidence
changes conversion units; missing required fields fail closed. Build-tier submit
verified zero open items (list 70). The deep tier was not run.

## Checkpoint: error/apply graph-closure uses

Both loaders collect candidate-independent written-use evidence in the required,
snapshot-bound `SourceErrorApply` inventory. Apply selects its tracked names from
that evidence rather than running another syntax visitor. This includes located
identifiers and struct/enum declarations, path prefixes outside inline modules,
macro identifiers, cfg blockers and conservative textual mentions.

Path records are sorted and deduplicated because graph closure consumes membership
rather than visitor order. Native collection preserves raw names, character-based
columns, qualified-self paths, shorthand fields, documentation attributes and typed
receiver paths. Macro inputs stay opaque: their identifiers block transformations
without becoming ordinary uses.

Expression, getter, initializer-expression and lifetime ports still remain before
production cutover, together with Cargo-context and compatibility-owner retirement.

Validation on 2026-11-05: the 22-case located-use parity fixture and complete
safety parity pass with full features, alongside all 47 error-apply regressions.
Consumer mutations verify that cfg blockers, macro mentions and path records
affect graph plans; missing required evidence fails closed. Tracing apply resolved
the two new instrumentation findings, and build-tier submit verified zero open
items (list 72). The deep tier was not run.

## Checkpoint: error/apply graph expressions

Both loaders collect candidate-independent expression evidence into the required,
snapshot-bound `SourceErrorApply` inventory. Graph apply selects tracked types and
fields from this payload instead of visiting syntax. Records cover struct and
variant construction, storage conversion ranges, source methods, Kind access,
field uses, patterns, macro blockers and constructor lifetime origins.

The native traversal threads implementation and signature context through the
canonical tree. It retains exact byte ranges and distinguishes supported cloning
and boxing from expressions whose borrow origin is unknown. Formatting-macro
guards use loader-issued argument trees. The compatibility guard also consumes
the shared macro grammar, removing its private per-field argument parser.

Getter, initializer-expression and lifetime-structure ports remain, along with
Cargo-context integration and compatibility-owner retirement. This checkpoint
does not switch production ownership.

Validation on 2026-11-05: the 23-case complete expression parity fixture and
production-consumer mutation tests pass with `internal_error_chain` alone, as do
all 47 error-apply regressions. Missing expression evidence fails closed, and
removing expression events changes executable graph plans. Shared formatting
argument reuse is also covered. Full-feature parity, consumer and shared-source
checks pass as well. Build-tier submit verified zero open items (list 75). The
deep tier was not run.

## Checkpoint: error/apply graph lifetimes

Both loaders retain candidate-independent graph lifetime evidence in the required,
snapshot-bound `SourceErrorApply` inventory. Production apply selects tracked
names from this payload instead of visiting the compatibility syntax tree.
Records include declaration and generic ranges, retained field lifetimes,
implementation and method contracts, constructor anchors, safe forwarding
returns, located type uses and expression-generic blockers.

Native collection preserves module and signature scope, qualified-self paths,
trait-bound distinctions and exact byte ranges. Trait paths are not editable
type-use records; their nested generic arguments remain visible. Both frontends
sort type uses by source range to give the planner deterministic evidence.
Missing lifetime inventory fails closed rather than rebuilding it downstream.

Getter contracts and uses, initializer-expression and local lifetime planning
still need native collection. Cargo-context integration and retirement of the
compatibility source owner also remain before production cutover.

Validation on 2026-11-05: the 22-case complete lifetime parity fixture and both
production-consumer tests pass with `internal_error_chain` alone and with full
features. All 47 error-apply regressions pass. Missing lifetime inventory fails
closed; mutating declaration, implementation or expression-blocker evidence
changes executable graph plans. Build-tier submit verified zero open items
(list 78). The deep tier was not run.

## Checkpoint: error/apply getter storage contracts

Both loaders project required, snapshot-bound getter storage evidence in
`SourceErrorApply`. The planner consumes these records for generated alias
ownership, crate-wide collision counts, field/derive compatibility, conditional
Display guards and precise insertion/removal edits. It no longer scans the
compatibility syntax tree to reconstruct getter storage contracts. Consumer
identity and alias reservation remain crate-wide joins over the collected facts.

Native attribute arguments use canonical tokens and loader-issued, memoized
expression grammar. Conditional field attributes and generated doc markers keep
the same forward/reverse ownership checks. Identifier counts cover opaque macro
and attribute tokens without counting identifier-like text inside string literals
or documentation.

Getter consumer collection, initializer expressions and local lifetime planning
remain, followed by Cargo-context production integration and retirement of the
compatibility syntax owner. This checkpoint does not switch production ownership.

Validation on 2026-11-05: complete getter-payload parity passes with full features
and with `internal_error_chain` alone. Both IR-consumer tests pass with full
features, along with all 14 getter compilation and
round-trip regressions. Missing getter inventory fails closed; removing contracts
or adding a reserved alias to the projected name counts changes executable plans
without changing source snapshots. Build-tier submit verified zero open items
(list 79). The deep tier was not run.

## Checkpoint: error/apply getter consumers

Both loaders project required, snapshot-bound getter consumer recipes and method
contracts in `SourceErrorApply`. Crate-wide identity joins resolve these records
without visiting source syntax. The native collector preserves lexical binding,
shadowing, trait-default and conditional-method guards, UFCS uses and exact edit
locations. Opaque standard and amenable harness payloads use loader-owned memoized
grammar queries, with relative origins for nested and repeated identical payloads.

Initializer expressions and local lifetime planning remain before Cargo-context
production integration and retirement of the compatibility source owner. This
checkpoint does not switch production ownership.

Validation on 2026-11-05: complete getter-consumer payload parity passes with full
features and with `internal_error_chain` alone. Both IR-consumer tests and all
14 getter compilation and round-trip regressions pass. Required-payload rejection
and same-snapshot mutation checks exercise the executable apply consumer. Build-tier submit verified zero open items
(list 81). The deep tier was not run.

## Checkpoint: error/apply local lifetime contracts

Both loaders now project required, snapshot-bound local lifetime contracts in
`SourceErrorApply`. Production planning selects tracked names from these records
instead of scanning compatibility syntax. The payload retains constructor and
scalar-context constraints, permitted trait/inherent implementations, safe
forwarding returns, allowed type-use positions and exact insertion/removal ranges
for conversion in either direction. Rejected attributes, lifetime bounds and
incompatible signatures retain their existing guards.

Constructor-initializer collection and conversion-site dispatch remain syntax
consumers; the error/apply group is not complete. In particular,
`inventory/batch.rs` still walks the compatibility tree for each file and applies
candidate policy in `inventory/batch/visit.rs`. Its use of already projected
declarations and getter facts does not remove that remaining syntax dependency.

For initializer collection, project candidate-independent expression recipes and
constructor contexts: binding counts and shadows, caller-location aliases,
immutable string parameters/locals, impl lifetime compatibility, tuple file
origins, configuration branches, field attributes and source ranges. Apply should
select these facts using the candidate's field, lifetime and strategy, without
reading an AST. Keep opaque configuration grammar loader-owned and memoized.

For conversion-site dispatch, project located use recipes and return-scope
contracts, including From/source/try evidence, constructor and pattern paths,
aliases, generated-constructor guards and macro field uses. Preserve traversal
order, nested module boundaries, candidate-specific getter exclusions and
fail-closed behavior in the syntax-free join. Verify complete payload parity and
same-snapshot mutations of executable conversion units before retiring the visitor.

Cargo-context production integration and compatibility-owner retirement still
follow these ports. Production ownership has not switched.

Validation on 2026-11-05: the complete 50-case local lifetime payload parity
fixture passes with `internal_error_chain` alone. Both full-feature IR-consumer
tests and all 47 error-apply regressions pass. The dedicated executable-consumer
test passes in both directions: changing required lifetime sites, permitted type
uses, forwarding returns or edit ranges changes the generated plans without
changing source snapshots. Missing local lifetime inventory fails closed.
Build-tier submit verified zero open items (list 84). The deep tier was not run.

## Checkpoint: error/apply constructor initializers

Both loaders project required, snapshot-bound constructor-initializer recipes in
`SourceErrorApply`. Apply selects these records by candidate field, cfg attributes
and lifetime policy without visiting syntax. The native collector retains binding
counts and shadows, caller aliases, immutable string parameters and locals,
qualified constant dependencies, impl lifetime requirements and exact edit ranges.
Complementary cfg tuple branches preserve both caller-location and zero-location
origins. Canonical cfg syntax is read directly; opaque attribute grammar remains a
loader-owned memoized query.

Conversion-site dispatch in `inventory/batch.rs` remains an original-syntax
consumer. Cargo-context integration and compatibility-owner retirement follow;
production parser ownership has not switched at this checkpoint.

Validation: complete initializer payload parity passes for ordinary and rejected
expressions, binding shadows, lifetime contexts and complementary cfg branches,
including opaque delimiters. Required-payload and same-snapshot mutation checks
prove executable plans consume initializer records, lifetime requirements and
expression ranges from IR. The 47 error-field apply tests and 14 getter apply
tests pass. Build-tier submit verified zero open items (list 86). The deep tier
was not run.

## Checkpoint: error/apply conversion-site contracts

Both loaders project required, snapshot-bound conversion-site events in
`SourceErrorApply`. Candidate selection and crate-wide getter exclusions consume
these records through the IR index. The former batch syntax visitor is removed.
The payload retains constructor and pattern paths, associated-call callee guards,
module and impl scope, return contracts, From/source/try evidence, aliases,
generated-constructor guards, macro field uses and exact source positions.
Candidate-independent collection preserves traversal order and keeps macro
argument grammar loader-owned.

Cargo-context production routing and compatibility-owner retirement remain.
This port does not switch production parser ownership or make handwritten
resolution authoritative for new semantic transformations.

Validation: complete conversion-event and source-error/apply payload parity
passes across scoped returns, generic signatures, source methods, macro guards,
aliases and generated attributes. Materialized type indexes, graph plans and
conversion units agree; same-snapshot event mutation changes executable units.
Missing conversion payloads are rejected. The 47 error-field apply tests,
14 getter apply tests and both projection-consumption tests pass. Adversarial
macro coverage also corrected the native expression collector's formatting
exemption: qualified and unrelated macros remain unresolved.
Complete expression payload parity passes after that correction. The final
conversion-site parity run also passes after the module split. Build-tier
submit verified zero open items (list 88); the deep tier was not run.

## Checkpoint: error/apply consumers use identities and text

Error/apply enrichment and planning now receive `FileSnapshot` identities from
`SourceErrorApply`, rather than compatibility syntax handles. The inventory's
file universe, namespace joins, getter identities, conversion selection and
graph plans come from retained IR records. The second directory walk and parse
handle acquisition in error enrichment and apply are removed.

Edit views contain snapshot identity and database-owned frozen text, without
retaining an AST. Issuing a view checks the text digest against its IR snapshot;
source-write checks still reject changed disk inputs. The session database can
seed unseen text from the loader's frozen bytes without parsing it.

Validation covers complete native/compatibility conversion payloads and plans,
47 error-field apply tests and 14 getter round-trip tests. Projection-consumption
tests assert that materialization never requests a compatibility parse, uses
frozen loader bytes despite later disk changes, ignores newly added files and
rejects mismatched fresh snapshots. Build-tier submit verified zero open items
(list 89); the deep tier was not run. Compatibility collection remains until
production workspace routing and ownership retirement are complete.

## Current next step

Load the Cargo context into the production session-owned database and route
source collection through its native projection. Error/apply consumers now use
IR identities and text without compatibility syntax carriers. Derives,
visibility, CLI layout, module splices, attribute materialization and the complete
file-policy and tracing groups now consume loader projections.
Preserve inactive-code policy coverage and golden findings.
Integrate the Cargo context driver before using semantic facts to authorize
transformations. Track the remaining
collector groups, context gaps and ownership removals against the active
milestone; do not treat each port as a separate product destination. Continue
implement / focused checks / build-tier submit / commit. Keep the selected
architecture as the destination throughout; do not create another user-facing
backend choice or extend handwritten semantic resolution.

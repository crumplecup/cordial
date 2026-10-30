# Strategy transformations through `--apply`

**Status:** Active design; implementation has not started.

This plan defines the broader purpose of strategy-driven apply and a first
implementation path using panic-to-error migrations. The
[error-field apply roadmap](error-field-apply-roadmap.md) retains the verified
Owned/Borrowed coverage and its paused backlog. That work supplies useful
infrastructure and lessons; it is not the implementation of this plan.

## Motivating purpose

Cordial should help a program adopt a coherent design. A strategy selects among
competing patterns that solve a problem in different ways. Owned and Borrowed
storage are one example. Returning errors and panicking are another. Typed
umbrella errors, erased error objects, and alternative error-construction or
diagnostic libraries supply further choices.

Today, an agent can fix one lint, expose the next missing piece, submit again,
and repeat until it gradually discovers a design Cordial already knows. Each
local repair may be plausible while leaving the program poorly organized as a
whole. Repeated agent idioms can become local maxima: convenient at the call
site, expensive or inconsistent across the program.

The opportunity is to encode known transformations between these designs.
Lints describe disagreements with the selected strategy; apply uses the same
evidence to plan the related changes together. An agent should be able to
review and apply a known migration instead of reconstructing it through ten
rounds of findings. Manual work remains for unresolved evidence and actual
design decisions.

The product goal is **reliable construction of a selected design from ordinary
code**, including code Cordial has never transformed. Coverage is measured by
the patterns and complete programs it can migrate safely, not by the number
of individual text substitutions.

## Central design decision: start from panic

The Owned/Borrowed experiments sometimes retained information from the richer
owned form so it could be restored from the borrowed form. That is valuable
preservation behavior, but it must not become a prerequisite for constructing
the richer design.

Start with independently authored, minimal panic-based programs. They have no
Cordial migration markers, generated aliases, retained error enum, or earlier
typed-error checkpoint. Derive available facts from their source and obtain
design choices from the selected strategy. Generate the missing structure.

Panic is a useful starting point because little error architecture is present:
an `unwrap` may expose a typed failing operation, but there may be no umbrella
error, propagation signature, source wrapper, or suitable caller boundary.
This forces the implementation to solve structure construction at every step.
It does not imply that propagating signature changes through Rust is trivial.

The generated type aliases used for derived borrowed getters illustrate the
principle: when a target idiom needs an additional declaration, generating that
declaration is a legitimate part of the transformation. The same applies to
error variants, conversions, and changed function signatures.

### Two independent capabilities

1. **Construction:** panic-based source → a coherent typed-error design without
   historical metadata. This drives the architecture and first acceptance test.
2. **Preservation:** richer design → simpler design → richer design while
   retaining supported details. Additional metadata can improve fidelity, but
   absence of metadata must not disable generic construction.

Shared IR facts belong to the current source snapshot. After a fresh process
or reload, construction must still work from the source and target policy.
Do not rely on an old IR cache as an archive of deleted program structure.

## What a strategy selects

Represent choices at the scope where they compete. Not every library is an
alternative at the same level, and not every choice is project-wide.

| Dimension | Illustrative choices | Consequence |
| --- | --- | --- |
| Failure behavior | Panic, return an error; abort may be a later distinct policy | Changes whether a caller can recover and where failure terminates. |
| Error representation | Typed umbrella enum, erased error object | Determines variants, conversion boundaries, and source traversal. |
| Storage | Owned, Borrowed | Determines value origins, allocation, and lifetime propagation. |
| Construction and diagnostics | Hand-authored or library-supported patterns; miette, anyhow, snafu where applicable | May affect derives, context, reporting, dependencies, and public interfaces. These libraries overlap in responsibilities and are not a single interchangeable enum. |
| Boundary behavior | Propagate, report, intentionally panic | Defines where a migration stops and how failures are surfaced. |

These are conceptual dimensions, not proposed TOML keys. Initial implementation
must fit the existing configuration path and identify unsupported combinations
explicitly. Selecting a panic strategy is a legitimate policy choice, including
for appropriate embedded programs. `no_std`, allocation availability, panic
unwinding versus abort, and target constraints still need distinct evidence.

The existing [`Strategy`](../../src/plugin.rs) trait selects participation in
a plugin's etiquette portfolio; it is not currently a transformation protocol.
Prototype against the first complete migration before fixing a generalized
public trait or adding another policy framework.

## Transformation contracts

### Preserve what the selected policy does not change

Owned/Borrowed can preserve observable behavior within a proven conversion
unit. Panic → returned error intentionally changes failure behavior. Describe
that as a policy change, while preserving successful results, successful-path
side effects and their order, error identity where available, and unaffected
interfaces. On the failure path, identify the intended change and preserve
context and source information required by the target strategy.

Unwinding, destructors, caught panics, panic hooks, and boundary diagnostics can
make termination behavior observable. A refusal must name an unsupported
interaction; compilation alone does not establish behavioral equivalence.

### Failure sites have different evidence

| Starting site | Available evidence | Required target structure |
| --- | --- | --- |
| Proven `Result<T, E>::unwrap()` | Success type and typed source `E` | Preserve `E` through the selected error chain; propagate the failure. |
| Proven `Result<T, E>::expect(message)` | The above plus panic context | Preserve or explicitly account for the message's value, evaluation, and side effects. Do not silently drop it. |
| Proven `Option<T>::unwrap()` | Absence, without an existing source error | Generate a policy-appropriate domain variant and its construction. Do not invent a source error. |
| `panic!` in an explicit failure branch | Branch condition and message/context expressions | Generate a domain failure and early return while preserving evaluation constraints. |
| `unreachable!`, assertion, indexing, arithmetic panic, or custom panic-like method | Often an invariant claim or implicit operation | Needs its own contract. Do not assume it is recoverable or mechanically replace it with `?`. |

Only classify `unwrap` from a resolved receiver/result contract. A same-spelling
user method is not evidence. `compile_error!` is a compile-time failure, not a
runtime panic strategy site.

### Reverse transformation

Typed errors → panic can remove propagation and choose `unwrap`, `expect`, or
an equivalent branch according to the selected contract. It still requires
whole-call-graph planning, correct successful return types, and intentional
boundary behavior. Do not remove a shared error declaration, dependency,
lifetime, or import until remaining uses establish that it is unused.

Exact reconstruction of a prior user-authored error taxonomy is a stronger
promise than generating a valid new taxonomy. Preserve recoverable details
where useful, record what was synthesized, and test these promises separately.
In particular, an `expect` message is not a serialization format for an error
graph. Generated-source metadata must be validated against current declarations
and consumers; it cannot authorize an edit by itself.

## Saturation: evidence and program structure

Saturation has two related jobs:

1. **Evidence saturation:** collect and connect the facts needed to decide the
   migration. Resolve declarations, signatures, failing operations, callers,
   return paths, and boundary constraints in the shared IR. Keep unknowns
   explicit; absence of an edge is not proof that no caller exists.
2. **Structure saturation:** plan the declarations and signature changes needed
   by the target pattern. Generate error types, variants, source wrappers,
   conversions, and successful-return adapters where their contracts require
   them. A previous richer version of the program is unnecessary.

The process is driven by the target strategy. It is not permission to generate
arbitrary scaffolding until lints stop firing. Each generated element must
discharge a named obligation and have a known consumer.

### Pipeline responsibilities

| Stage | Responsibility |
| --- | --- |
| Shared loader | Issue snapshot-bound source text, syntax, identity, and digests. Parse supported macro Rust islands once through the same loader. |
| Collection | Record function and type declarations, explicit signatures and return paths, failing expressions and their spans, bindings, calls, imports, cfg, macro contracts, and source context. |
| Enrichment | Resolve callees and source types; connect caller/callee, error conversion, and boundary edges; classify provenance and unresolved constraints. Store reusable evidence in the shared IR. |
| Strategy planning | Query `IrView`, choose the target representation, close the affected component, and solve required return/error types and generated declarations. Record deterministic edits and blockers. |
| Apply | Read located text and digests through `SourceEditView`, reject stale or conflicting edits, stage the complete component, and use the existing transaction/rollback machinery. |
| Validation and reporting | Check the selected contract, report planned/applied units and residual blockers, update matching worklist claims after successful writes, and let submit verify those claims. |

One shared parse and one shared IR remain mandatory. Missing facts are added
to collection/enrichment, never recovered by an apply-specific parser, source
scan, private call graph, or side cache. Compiler/rustdoc facts, if needed for
resolution, must enter through an explicit loader/enricher path and become
shared IR evidence. Tests compiling transformed code do not substitute for
pre-edit resolution evidence.

Proposed declarations and obligations may be represented as a typed plan
overlay linked to original IR identities. This is planned output, not a second
analysis database. Reuse the existing IR/edit machinery rather than building
parallel function, type, or ownership indexes.

## Reusable transformation operations

Build concrete operations needed by the examples, then generalize their shared
contracts. Candidate operations include:

- Lift a function from `T` to `Result<T, E>`; wrap its successful tail and every
  successful early return, preserving `return`, `break`, and `?` scope.
- Convert a proven failure expression to propagation or a constructed domain
  failure; preserve its source and required context.
- Introduce or extend an error declaration, source wrapper, variant, conversion,
  import, and trait implementation under a deterministic naming policy.
- Adapt every affected caller and explicit function-type use. Resolve its own
  return obligation or terminate at a selected boundary adapter.
- Change an existing error type, merge required source alternatives, and
  connect the selected conversions without erasing sources or creating
  ambiguous `From` implementations.
- Reverse propagation, unwrap successful results, and remove generated
  structure only after proving its consumers are gone.

Each operation needs typed preconditions, affected IR identities, intended
behavioral change, generated obligations, edits, and postconditions. Store
these obligations as structured facts rather than strings interpreted later.
Names and imports must be collision-safe and stable on repeat apply.

### Compose before writing

A leaf return-type change creates obligations at callers. Callers may combine
several failures, belong to recursive components, or be constrained by a trait
signature. Plan the complete affected component before writing it.

Use a work queue over resolved dependency edges to reach a stable set of
signature and error-type obligations. Treat recursive strongly connected
components together. Track discharged and unresolved obligations, detect
incompatible requirements, and stop rather than alternate indefinitely between
strategies. The selected target gives the direction; partial saturation does
not trigger another quality run to discover the next mechanical step.

Do not write an intermediate program that merely exposes the next lint.
Independent closed components may proceed; one unresolved edge blocks its
affected component without forcing unrelated units to fail.

### Boundaries requiring explicit contracts

Collect trait implementations, closures and callback signatures, function
pointers, generics, recursion, async/await, public/external callers, macros,
cfg-specific callers, and const contexts even before supporting their rewrites.
Record a located blocker for an unsupported boundary. Do not silently omit it.
For `main` and tests, define a supported propagation/reporting adapter; for
fixed trait signatures or foreign callbacks, either use a proven local adapter
allowed by policy or refuse the component. Never change an external contract
merely to make a local transformation compile.

Public API changes must be identified in the plan, with their intended scope
and compatibility consequences settled before implementing that coverage.
Whole-workspace migration and unknown downstream consumers are different
closure conditions. Embedded targets additionally require allocation and panic
handler constraints to be represented before claiming support.

## Executable examples as specifications

Use Cargo's **example binary targets**, not a Cargo feature named `examples`.
The repository already has `examples/custom_plugins/`. Add focused binaries
such as `examples/apply_panic_result/main.rs` and later multi-file examples.
Cargo can run a specimen with `cargo run --example apply_panic_result`.
Use explicit `[[example]]` entries only where target metadata requires them.

Examples are readable, runnable input programs, with real behavior and small
dependencies. An integration test under `tests/` copies a specimen into an
isolated fixture, applies the selected strategy there through the production
pipeline, then compiles and executes the changed program. The checked-in
example stays unchanged. Keep test orchestration outside the specimen.

The copied fixture needs an explicit Cargo manifest, source roots, and policy
so the ordinary shared loader analyzes its executable target and modules.
Audit target discovery: an example residing outside `src/` must not produce an
empty successful transformation. Share the specimen source with the test;
do not maintain a second inline string version that can drift.

Deliberately panic-based specimens must have an explicit fixture role in the
scan/test layout. Preserve normal Cordial standards for production examples;
do not add blanket lint suppression or self-approved exceptions to accommodate
test inputs. If target/scoping support is missing, include that in the first
phase rather than hiding input from the transformation itself.

### First specimen and growth matrix

Start with a private leaf calling a proven `Result`-returning operation,
unwrapping it, and returning its successful value to one private caller. Give
the executable a defined success/failure scenario and boundary. A helper with
an explicit resolved `Result` signature can make the first source type
tractable; it must not supply the generated umbrella architecture in advance.

The typed target must introduce its error structure, change both signatures
where required, preserve the original source, and handle the executable
boundary. Finishing only an `unwrap` substitution does not finish this slice.

| Specimen family | New obligation | Required assertions |
| --- | --- | --- |
| Result leaf and one caller | Source preservation, generated error, lifted returns, boundary | Same successful value/output; typed failure and reachable original source; valid executable. |
| Multiple returns and failure sites | Branches, early returns, multiple source types | Every path adapted; evaluation counts and side-effect order preserved. |
| Option absence and explicit panic branch | Synthesize a domain error without an original source | Stable variants and preserved context; no fabricated source chain. |
| Expect context | Message construction and diagnostics | No lost or spuriously evaluated context; intentional diagnostic differences stated. |
| Several callers and cross-file paths | One coherent dependency component | All callers included; one unresolved caller prevents all writes to that component. |
| Recursive callers and existing Result functions | Joint signature solution and error merging | Stable plan; no conflicting conversions or repeated enum growth. |
| Fixed boundaries and macro/cfg uses | Adapters or conservative refusals | Located reason; unchanged source/worklist on refusal. |
| Library and ownership alternatives | Compose representation/storage/library choices | Supported combinations preserve their stated contract; unsupported combinations are explicit. |

Add an independently authored typed-error specimen for the reverse direction.
Test a fresh construction after historical metadata is absent, not only an
undo of generated source.

### Test protocol

1. Compile/run the original input. Establish successful outputs, side effects,
   and expected panic behavior on designated failures. Use subprocess outcomes
   where unwinding assumptions would be inappropriate.
2. Preview under the target strategy. Assert that source and worklist are
   unchanged and inspect the complete plan, boundary, and generated structure.
3. Apply through the production shared-IR path. Assert preview/write agreement
   and that only corresponding completed findings are claimed.
4. Compile/run the transformed program. Check successful-path equivalence,
   target failure behavior, source traversal, and caller-location policy.
5. Repeat apply: no edits, no duplicate declarations, and no extra claims.
6. Exercise the reverse direction and its contract. Separate fresh construction,
   behavioral round trips, and exact source restoration where promised.
7. Exercise lookalikes, unresolved callers, name collisions, stale snapshots,
   and edit failures. Reject the affected component without partial writes.

Expected-output snapshots help review generated code but cannot replace
compilation and execution. A lower finding count cannot replace these checks.
Keep the first examples small enough for routine focused Cargo tests; reserve
large-library experiments for validating newly demonstrated coverage families.

## User workflow and lint integration

The selected configuration expresses the destination design. Keep one normal
configuration route; do not introduce hidden environment overrides or a
parallel migration policy file. Exact new option names and CLI entry points
remain implementation decisions after inspecting the existing policy API.

Use the existing apply/dry-run conventions. A preview should expose the source
pattern, target strategy, affected declarations/files, generated structure,
signature/API effects, intended failure-policy change, and located blockers.
An agent must be able to understand the plan without parsing tracing output.

After successful writes, claim only findings whose repair corresponds to
applied units, as tracing and error-field apply already do. A dry run, refusal,
stale snapshot, or failed write must not check anything off. Count completed
units, resolved findings, and remaining manual findings separately. Submit
verifies the claims; apply does not declare all quality gates passed.

Strategy-aware lint activation must agree with the selected target. An
intentional panic policy must not immediately be contradicted by a lint that
unconditionally demands typed propagation. Conversely, choosing panic cannot
silence unrelated problems. Current `cordial explain panics` describes the
existing error-first policy, not an implemented panic alternative. Update
compiled explanations with actual strategy support as it lands.

## Implementation roadmap

- [ ] **Phase 0 — executable specification and discovery.** Add the first Cargo
  example and integration-test harness. Prove the production loader inventories
  its leaf, caller, failure site, source type, and boundary. Document fixture
  scoping. Preserve a compiling rejection example. No claim of apply coverage
  until an actual transform passes its executable contract.
- [ ] **Phase 1 — first complete construction.** Add missing typed IR evidence,
  target-strategy selection, generated umbrella/source structure, return
  lifting, caller adaptation, and executable boundary handling. Deliver one
  minimal panic → typed-error example end to end, without retained metadata.
  The dry run presents one complete plan; the writer commits one complete unit.
- [ ] **Phase 2 — reusable return and failure operations.** Cover branches,
  early returns, multiple typed sources, Option absence, and explicit panic
  failures through the shared operations. Add expect only with its context
  contract. Validate negative cases and deterministic naming.
- [ ] **Phase 3 — composition and wider closure.** Extend to several callers,
  modules, existing Result surfaces, and recursive components. Add each fixed
  signature, macro, cfg, or async boundary only with collected evidence and
  executable fixtures. Preserve atomic refusal for unknown edges.
- [ ] **Phase 4 — reverse construction and preservation.** Transform independently
  authored typed-error code to panic, then construct typed errors afresh.
  Preserve extra details where supported; metadata-assisted exact restoration
  remains an additional capability rather than the core proof of construction.
- [ ] **Phase 5 — real programs and additional strategies.** Survey isolated real
  projects, map new failures to specimen families, and measure manual versus
  automated units. Extend toward ownership and alternative-library strategies
  using the same operations and shared evidence. Handle dependency/feature
  changes as explicit parts of the unit. Do not promise every pair of libraries
  is automatically interchangeable.

Each phase follows implement → focused checks →
`cordial submit --tier build --deny-open` → scoped commit, resolving build-tier
findings before committing. Use normal default stores, no exceptions, and
`just install` only when binary behavior changes. Keep expensive all/deep-tier
verification for the end of the plan. Use existing tracing/hotpath facilities
to measure collection, enrichment, planning, and writing when assessing cost;
do not add duplicate analysis to collect timings.

## Success criteria

- A fresh panic-based executable can acquire a coherent typed-error design in
  one apply plan, without historical metadata or iterative lint discovery.
- Added structure has a specific policy purpose, typed evidence, and known
  consumers. The plan accounts for all affected signatures and callers.
- All analysis uses the shared loader and shared IR. No transformation family
  establishes its own parser, call graph, or durable source cache.
- Examples are executable specifications with construction, rejection,
  idempotence, and reverse coverage. Compilation is necessary; observed behavior
  and explicit contracts establish the rest of the evidence.
- Unknown boundaries produce useful refusals without partial migrations.
- Existing Owned/Borrowed and tracing apply remain supported. The paused
  Amenable roadmap retains its measured coverage and independent backlog.
- Agents spend fewer submissions discovering known mechanical steps, while
  remaining design decisions and unsupported cases stay visible.

## Decisions to settle during implementation

Settle these against the first example, not by inventing a universal API first:
the precise strategy config and scope; stable declaration/site identities for
signature propagation; source-type resolution evidence; generated error naming
and placement; entry-point adapters; expect-context preservation; and the
public contract for a composed transformation plan. Record each decision here
as its first executable proof lands.

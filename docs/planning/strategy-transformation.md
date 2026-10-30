# Strategy transformations through `--apply`

**Status:** Shared-IR foundations, isolated operations, transitive composition,
and closed single-module binary migrations through a unit-main adapter are
implemented and tested against executable specimens. Closed migrations now
support guarded dry run and writes through the shared source transaction and
`quality --apply --etiquette panics`, with exact worklist claims. Broader use
and target forms remain explicit refusals.

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
   historical metadata. This drives the architecture and first composed
   acceptance test, after the constituent transforms pass in isolation.
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

### Prove individual transforms before composing them

Build complexity incrementally. Collection and enrichment can anticipate the
evidence needed by the larger migration, but transform implementation and tests
must advance one operation at a time. A broad IR model does not require a broad
first rewrite.

Start with independently testable operations: generate a declaration, lift a
return signature and its successful paths, rewrite a proven failure site,
adapt a caller, and handle an executable boundary. Give each its own minimal
input, typed preconditions, expected edits, compiling target, behavior checks,
and relevant refusal cases. An isolated fixture may supply the surrounding
target structure needed to exercise that operation; it proves that operation,
not construction of the whole design from panic.

Once the individual operations are established, compose the smallest compatible
pair and verify their combined obligations. Grow through a leaf migration,
one caller, and a boundary before adding branches, multiple sources, additional
callers, or recursion. Each new example should introduce one new obligation
where practical, making failures attributable to the new capability.

Keep the independently authored panic-to-typed-error executable as the first
complete construction milestone. It is a destination for the initial operations,
not the first implementation task. Isolated development does not authorize
writing an incomplete migration to a user's program: each exposed apply unit
must still be closed and atomic.

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

### Isolated specimens and the first construction milestone

Begin with small runnable specimens for the individual transforms described
above, then add specimens for their smallest compositions. Use the same
production shared-IR and edit paths for these tests; do not substitute a
test-only text-rewrite implementation. Apply the relevant checks from the test
protocol below to each operation, adding chain-wide checks as composition grows.

For the first complete construction milestone, use a private leaf calling a
proven `Result`-returning operation,
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

- [x] **Phase 0 — executable specification and discovery.** Add isolated Cargo
  specimens and an integration-test harness. Prove the production loader
  inventories the declarations, signatures, failure sites, source types,
  callers, and boundaries required for the initial operations. Document fixture
  scoping and add missing typed IR evidence through collection/enrichment.
  Preserve a compiling rejection example. No claim of apply coverage until an
  actual transform passes its executable contract.
  - [x] Add runnable input specimens for return lifting, a Result panic chain,
    and a same-spelling user method; compile and execute their baselines.
  - [x] Copy actual example bytes into an explicit isolated Cargo binary target;
    assert production target/function/panic discovery and source immutability.
  - [x] Retain snapshot-bound free-function signatures, declared output syntax,
    and signature/body byte ranges in shared IR, with strict typed reads.
  - [x] Collect return paths and nested execution scopes for isolated return
    lifting. Retain output edit ranges and explicit signature, attribute, macro,
    residual, and nested-scope boundaries before planning edits.
  - [x] Resolve source Result contracts and caller/boundary relationships through
    shared enrichment. A declared output string is not resolution evidence.
- [x] **Phase 1 — individual transforms.** Implement and test declaration
  generation, return lifting, failure-site rewriting, caller adaptation, and
  boundary handling separately, in dependency order. Establish each operation's
  evidence, edits, behavior, idempotence, and refusal contract before adding it
  to a chain. Supply surrounding structure explicitly in isolated fixtures.
  - [x] Plan and execute isolated successful-return lifting from shared IR:
    explicit returns and a separate tail, deterministic preview, stale/corrupt
    evidence rejection, fixed-boundary refusal, and repeat planning without
    nesting the generated wrapper.
  - [x] Extend structural tail handling to returns nested in branches/blocks and
    prove unit fallthrough and evaluation counts before composing operations.
- [x] **Phase 2 — incremental composition and first construction.** Combine the
  proven operations in the smallest compatible steps: pairs, a complete leaf,
  one caller, then an executable boundary. Add target-strategy selection as
  needed. Finish with a minimal panic → typed-error executable without retained
  metadata or pre-supplied error architecture. The dry run presents one complete
  plan; the writer commits one complete unit.
  Completed for the documented initial owned, single-I/O-source, closed
  private binary domain. Broader strategies and source contracts remain below.
- [ ] **Phase 3 — richer return and failure patterns.** Cover branches,
  early returns, multiple typed sources, Option absence, and explicit panic
  failures through the shared operations. Add expect only with its context
  contract. Validate negative cases and deterministic naming.
- [ ] **Phase 4 — wider closure.** Extend to several callers,
  modules, existing Result surfaces, and recursive components. Add each fixed
  signature, macro, cfg, or async boundary only with collected evidence and
  executable fixtures. Preserve atomic refusal for unknown edges.
- [ ] **Phase 5 — reverse construction and preservation.** Transform independently
  authored typed-error code to panic, then construct typed errors afresh.
  Preserve extra details where supported; metadata-assisted exact restoration
  remains an additional capability rather than the core proof of construction.
- [ ] **Phase 6 — real programs and additional strategies.** Survey isolated real
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

### Phase 0 discovery decisions

The [specimen guide](../../examples/STRATEGY_TRANSFORMS.md) records each input's
role and expected behavior. `tests/strategy_specimens.rs` compiles and runs the
copied programs and reads the normal session's serialized IR through `IrView`.
Source normalization into `src/main.rs` is explicit: direct analysis of native
Cargo example target paths is not yet supported. These are executable inputs,
not a test-only rewrite engine or a claim of apply coverage.

The shared source loader now attaches `FunctionSourceSyntax` to its existing
top-level free-function nodes. Collection selects the item from a
`SharedParsedFile`; it cannot be handed an independently parsed function.
The facts include the snapshot digest, exact signature and declared output
syntax, and byte ranges for signature/body edits. Reads distinguish missing
older evidence from corrupt evidence, which is an error. No new parse, private
source index, or durable side cache is introduced.

This does not establish inferred types, return-path coverage, method inventory,
inline-module inventory, or caller closure. The lookalike example deliberately
exposes the existing panic lint's syntax-only classification. Before the first
return-lifting transform, collect explicit returns, tail exits, and nested
scope/boundary constraints in shared IR. Before rewriting failure sites, add
resolved receiver/source contracts; the lint finding alone cannot permit an
edit. Continue with isolated operation tests before composing the panic chain.

### Return evidence (first follow-up cycle)

The loader now retains explicit returns, tail expressions or unit fallthrough,
output edit ranges, and nested execution scopes in `FunctionSourceSyntax`.
Collection walks the existing shared AST. Returns inside closures, async blocks,
inline consts, and local items do not leak into the enclosing function's return
inventory. Boundary facts explicitly identify visible/special/generic signatures,
attributes, macros, residual propagation, and nested scopes. They are constraints,
not proof that every function without one is a closed migration component.

`apply_return_scopes` compiles and executes both branches and exercises those
scope distinctions through the production loader and serialized IR. Caller
resolution and typed Result source resolution remain outstanding. The next
operation is isolated return lifting; it must not be exposed as a complete
user-facing migration before caller closure is available.

### Isolated return lifting (second follow-up cycle)

`plan_infallible_return_lift` consumes `IrView` and an existing function identity.
It emits a typed operation plan or a typed refusal; downstream planning does
not inspect a syntax tree. The first destination uses `Infallible` to test the
success adapter independently of source-error taxonomy generation. Exact
wrapper syntax is recognized during shared collection for repeat planning;
this is not receiver/type resolution evidence for rewriting panic sites.

A preview verifies the source digest, validates byte ranges, and renders located
edits without writing files or checking off findings. The executable specimen
uses Debug at its boundary so both the original value and its lifted Result can
be observed without bundling caller adaptation into the test. Both early and
fallthrough values are compiled and executed after the actual production plan
is rendered. Recollection returns `AlreadyLifted` instead of adding wrappers.

This is a library planning primitive, not a new user-facing apply mode. Its
caller obligations remain open. Macros, residual propagation, loops, nested
scopes, attributes with semantic effects, visible/special/generic signatures,
and never outputs are refused. Existing apply writers and closed component
transactions will be used when these operations can be composed.

### Structural success exits (third follow-up cycle)

The shared collector now retains `FunctionSuccessExit` facts for value leaves,
bare returns, block unit fallthrough, and an if expression's implicit else.
Branch, match, block, and parenthesized value paths are decomposed during
collection. The planner consumes those facts through `IrView`; it does not
reconstruct control flow from source text. A syntactically returning branch
followed by a semicolon does not acquire unreachable unit fallthrough.

`apply_return_paths` is the executable contract. Its tests lift each function
independently, compile without warnings, and execute every branch and match arm.
They cover nested explicit returns, implicit unit, unit fallthrough, and
mutation counts that verify once-only evaluation and order. Recollection proves
repeat planning is idempotent. Missing, duplicated, or malformed exit evidence
and invalid UTF-8 edit positions are rejected before any write.

**Next substantive operation:** collect and resolve source Result contracts and
caller/boundary relationships in the shared IR, then implement a failure-site
rewrite in isolation. The existing panic inventory identifies syntax, including
the same-spelling user method specimen; it does not authorize a rewrite.
Declaration generation and caller adaptation still need their own executable
contracts before composing a complete fresh panic-to-error migration. This
return primitive remains a library preview operation with no CLI writer or
worklist claims; it must not publish a partially converted program.

### Result and call evidence (fourth follow-up cycle)

The shared function facts now retain explicit absolute standard Result contracts,
their success/error syntax, direct call ranges, and zero-argument unwrap receiver
ranges. Collection uses the same `SharedParsedFile`; downstream consumers need
no syntax tree. Standard nongeneric absolute error paths have comparable type
identities. Relative paths, aliases, and generic error types remain unresolved.
Bound parameters and local patterns cannot acquire direct-function identities
merely by spelling a function's name. Calls within nested execution scopes do
not leak into the enclosing function inventory.

`apply_failure_site` isolates propagation inside an already fallible function;
its baseline compiles and runs with both successful and panicking inputs.
`strategy_result_contracts` checks persisted evidence and unresolved cases.
Next, join calls to source declarations and retain explicit resolution failures
and incoming caller relationships. Namespace ambiguity and source boundaries
must be checked before these declaration facts can authorize a rewrite.

### Shared call resolution (fifth follow-up cycle)

`SourceCallEnricher` now runs with the source loader and resolves eligible direct
calls against declarations in the same module's shared graph. Per-site evidence
retains the callee's node identity and snapshot digest. `Calls` edges retain
incoming caller relationships, with separate located records for repeated calls
and one adjacency edge per pair. Recursion is represented without traversing it.
The enrichment uses IR facts only, with a temporary declaration lookup built from
the graph; it neither parses nor keeps a parallel source inventory.

Unknowns are explicit: unsupported/qualified/indirect calls, bound names, missing
or duplicate declarations, and unsupported source contracts do not produce
resolved edges. Imports, namespace macros/attributes, local items, extern aliases,
and missing namespace facts constrain resolution. Absolute standard paths are
trusted only within that checked scope. Typed reads distinguish absent from
corrupt evidence. Tests inspect the persisted production graph, including
incoming edges, recursion, repeated sites, shadowing, and namespace refusals.

This resolver does not claim complete caller closure: method calls, function
values, macro expansion, nested functions, external callers, and cross-module
imports still need their own evidence. Next, use a resolved Result-producing
call and matching error identities to rewrite one unwrap inside an already
fallible function. That isolated operation must compile and execute before
combining it with success lifting, declaration generation, or caller adaptation.

### Isolated Result propagation (sixth follow-up cycle)

`plan_result_unwrap_propagation` now plans one proven Result receiver's `unwrap`
to `?` inside an already fallible function. It consumes only `IrView`: collected
method/receiver/token ranges, resolved source node identities, `Calls` edges,
namespace constraints, snapshot digests, and explicit Result/error contracts.
Caller and callee must have the same known absolute error identity; this first
operation neither infers `From` conversions nor resolves aliases. Ambiguous
standard-library names, including root declarations named `std` or `core`, cannot
authorize standard Result interpretation. Unknowns produce named refusals.

Both success lifting and failure propagation use `FunctionEditPlan` for located,
digest-checked previews. The existing return-lift API names remain aliases.
There is no second parser, text-based evidence recovery, CLI writer, or worklist
claim. Removing individual method tokens retains interspersed comments.

`apply_result_propagation` compiles and executes before and after the production
preview. Successful results, once-only source evaluation, and side-effect order
are preserved; failure returns the original typed `io::Error` instead of
panicking and skips the remaining successful-path computation. Tests also cover
same-spelling user methods, error mismatches/aliases, unresolved/shadowed calls,
namespace ambiguity, corrupted evidence, stale source, independent repeated
sites, and recollection without another candidate. Each repeated-site preview
is compiled separately, including the second operation after a `?` is present.

**Next independent contracts:**

1. Generate a typed error declaration and its source-preserving construction
   from a resolved failing source, without relying on prior migration markers.
   First compile and execute the generated declaration/conversion in isolation.
2. Lift an infallible function to that selected error contract and adapt one
   resolved caller independently. Keep public/trait/extern/unknown caller
   boundaries explicit; incoming `Calls` edges are not proof of closure.
3. Compose those proven operations into the smallest closed panic-to-error
   component, then connect the existing atomic apply writer and worklist claims.
   A preview of one operation must never be presented as a complete migration.

Cross-module imports, method/trait resolution, aliases, cfg/macro contracts,
Option/expect/domain failure construction, reverse transformations, and whole
caller closure remain uncovered. Extend shared collection/enrichment for each
missing fact before attempting its consuming transform.

### Owned source-chain construction (seventh follow-up cycle)

`plan_owned_error_declaration` constructs a fresh parent, boxed Kind, native
source wrapper, and caller-tracked `From` conversion from a resolved I/O failure
site. It needs no previous generated declarations or migration markers. Owned
storage is an explicit operation contract; borrowed construction remains a
separate lifetime obligation. The initial supported source is the known
`::std::io::Error` type, not arbitrary standard paths or Result error arguments.

The shared module facts now retain the source digest, end-of-file insertion
offset, and conservative identifier reservations collected from the same shared
parse. Planning consumes those facts through `IrView`, shares the existing
receiver/callee proof, and uses `FunctionEditPlan` for its preview. Names derive
deterministically from the source function and site index; a collision refuses
the operation, including raw identifiers and local bindings. Existing bytes
remain unchanged. Missing/corrupt module facts and stale source cannot authorize
an insertion. Recollection refuses duplicate generation.

`apply_error_declaration` is an independently authored panic specimen. Its test
supplies a surrounding consumer of the generated conversion in isolation, then
compiles without warnings and executes success and failure. It checks the
original successful result, downcastable native and foreign source links,
retention of a custom I/O payload, and exact caller file/line through both `?`
and direct conversion. This proves generated construction, not automatic
signature migration: the production preview only appends declarations.

**Next:** lift an infallible function to the generated error contract, then prove
one resolved caller adaptation independently. Compose only after both contracts
pass. Function values, public/trait/extern boundaries, cross-module resolution,
and unknown callers still prevent claiming a closed writable component.

### Generated error return lifting (eighth follow-up cycle)

`plan_owned_error_return_lift` lifts an infallible function's success paths to
the error contract selected by `plan_owned_error_declaration`. Both operations
consume the same shared IR snapshot. The declaration token carries its origin,
names, path, and digest; scope and namespace evidence must establish that the
consumer belongs to the same module. A caller cannot substitute an arbitrary
error type string. Existing Result contracts require a separate operation.

The shared success renderer now accepts the chosen error contract while
preserving the existing Infallible operation. `apply_error_return` independently
checks early and tail successes, then compiles and executes the lifted form with
the generated declarations. The original unwrap is deliberately unchanged in
this isolated test. Different modules, stale evidence, fixed signatures, and
already fallible functions refuse planning. No writer or worklist claim is added.

**Next:** prove adaptation of one resolved caller to a planned fallible callee,
then compose declaration, success lifting, and failure propagation. Caller
closure remains a separate obligation before connecting a complete migration
to the atomic writer.

### Isolated caller adaptation (ninth follow-up cycle)

`plan_owned_caller_adaptation` resolves a selected caller-to-callee relationship,
proves both can use the same generated error contract, and propagates every
resolved call to that callee within the caller. Its result retains the callee
signature lift as a required dependency. The declaration, success lifting, and
caller operation share one target/snapshot validator; no downstream source scan
or parser participates. Stable insertion ordering ensures `Ok(callee()?)`,
including exact-tail and early-return calls.

`apply_error_caller` executes every branch before and after the isolated plans,
checking values and call counts. Tests refuse fixed signatures, unresolved and
stale call evidence, including corruption in a later repeated site. The leaf's
unwrap remains unchanged in this independent contract test.

**Next: transitive composition.** A signature change propagates through callers
of callers; one adapted relationship is not a complete migration. Walk incoming
relationships to a fixed point, handle repeated edges and recursive components,
and retain explicit public/trait/extern/unknown-use boundary obligations. A
multilevel executable specimen must exercise that propagation in both success
and failure. Compose declaration generation, leaf failure conversion, success
lifting, and caller adaptation against one snapshot. No writer may treat
missing `Calls` edges as evidence of closure; function-value and unresolved-use
inventory still needs shared collection before a closed component can be claimed.

### Transitive panic-to-error composition (tenth follow-up cycle)

`plan_owned_panic_component` composes the independently tested contracts against
one shared snapshot: fresh owned I/O error declarations, leaf unwrap conversion
via the generated `From`, success lifting, and propagation through callers of
callers. An incoming-edge walk reaches a fixed point, deduplicates diamonds and
repeated edges, and terminates on recursive components. Every changed function
gets one signature lift; every located call between changed functions gets `?`.
Collected ranges authorize all edits, with propagation ordered inside success
wrappers and contradictory overlaps refused.

The preview has an explicit observation boundary whose own return type becomes
Result. Known caller paths terminating elsewhere refuse planning. This is not
proof of complete use-site closure: unsupported calls may lack `Calls` edges,
and a boundary's consumers may need adaptation. `PanicComponentPreview` exposes
no writer or worklist claim. Public/trait/extern/generic/macro boundaries and
additional panic sources remain named obligations rather than partial writes.

`apply_error_component` compiles and executes the complete rendered preview.
Seven functions cover multiple levels, repeated calls, a diamond, and both self
and mutual recursion. Successful values and evaluation counts are preserved;
failure returns the original typed I/O source through all levels, skipping
later computations at each level. Tests also check determinism, retained
comments, stale snapshots, a refused transitive signature, missing located
call evidence, and recollection with no remaining candidate.

**Next:** collect non-call function uses and unresolved-use boundaries into the
shared IR, then prove a closed component and an explicit boundary adapter
(starting with `main`). Only that closed contract may reach the existing atomic
writer and worklist claims. Generalize source variants and reverse transitions
after the forward closure and boundary contracts compile and execute.

### Written-use inventory (eleventh follow-up cycle)

The shared loader now collects value-path uses alongside namespace evidence in
one traversal of each shared tree. `SourceModuleScope::uses` retains written
segments, exact path and direct-call ranges, the owning top-level signature,
and nested execution boundaries. The walk includes function values, constants,
impls, inline modules, nested items, closures, async blocks, and inline consts.
Raw identifiers are normalized for name comparison while original ranges remain
unchanged. Macro and verbatim syntax are explicit opaque ranges.

These are written facts, not resolved identities. The next closure operation
must join them to enriched calls before authorizing edits, and refuse unresolved
matching names or opaque syntax. A missing inventory on an older snapshot is
unknown rather than an empty set. Corrupt inventory propagates an error.
Executable and persisted-IR tests demonstrate uses omitted by direct call edges
without adding another parse or a private analysis cache.

**Next:** consume this inventory to close a migration through a selected
executable boundary, then test a `main` Result adapter independently before
connecting closed plans to writing and worklist claims.

### Closed binary migration and main adapter (twelfth follow-up cycle)

`plan_closed_owned_panic_component` joins the written-use inventory to enriched
call identities and exact snapshot-bound call ranges. Every matching written
reference must resolve to a changed function and an adapted site within the
component; every changed call must have a corresponding written-use record.
Function values, nested/qualified unresolved uses, opaque syntax, missing
inventory, and inconsistent evidence refuse the entire plan. All collected
source modules participate, including files outside the changed component.
The resulting `ClosedPanicMigration` has no public constructor and is distinct
from the earlier preview, which does not prove closure.

The shared IR now retains binary entry paths from the existing Cargo metadata
load and a collected unit-main signature contract. Closure requires a single
binary target whose actual entry path and root function agree. Library/mixed
targets, multiple binaries, same-named non-entry functions, or older snapshots
without that evidence refuse. No new source scan, parse, or metadata side
channel is introduced. `plan_owned_main_return_lift` independently tests the
boundary's success contract before composition.

`apply_error_main` exercises eight transformed functions through the binary
entry point. Successful output and evaluation counts are preserved; failure
returns the typed I/O source through Rust's `Result` termination contract,
prints the generated Debug diagnostic, exits unsuccessfully, and does not
continue into successful-path output. This is an explicit boundary-policy
change from panic termination. Recollection finds no remaining candidate.

**Next:** connect the closed contract to the existing snapshot-checked atomic
writer, preserving dry-run and refusal behavior, then expose the complete
operation through apply and worklist reporting. Keep the initial supported
domain explicit: single-module private functions, one resolved I/O panic site,
and one binary unit main. Broader source/error variants, cross-module identity
resolution, and reverse transitions remain later work.

### Guarded migration writes (thirteenth follow-up cycle)

`apply_closed_panic_migration` accepts only the closed token, validates the
original source and manifest snapshots, renders the located edits, and stages
the complete single-file migration. Dry run performs the same checks without
creating temporary source files. Both modes report planned files, written
files, converted panic sites, and changed function contracts.

The token retains every source file used to prove closure, including unchanged
files, plus the manifest observed during Cargo target discovery. These inputs
are shared IR facts. Before writing, their digests and source-directory Rust
file membership must still agree. An untouched file edit, addition, removal,
manifest change, symlink, or replayed token refuses before replacement. The
directory walk checks membership only; it does not reconstruct source facts or
parse Rust. Concurrent external writes cannot be made fully transactional;
checks are repeated immediately before replacement.

The existing error-field writer now delegates to the same staging and rollback
implementation. Source permissions and original-file backups are preserved;
the existing multi-file error-field test remains a regression check. The new
writer tests compile and execute the actual written output, check failure
source preservation and successful output, and exercise stale proof inputs,
dry run, symlinks, and permissions. No CLI behavior or worklist claims have
changed in this cycle.

**Next:** route supported panic findings through this closed writer in the
existing apply workflow, report refusals and effects, and claim only matching
worklist items after real successful writes. Keep the initial single-module,
one-I/O-site, private-function, binary-unit-main contract explicit. Wider
closure, additional error alternatives, and reverse construction follow.

### Production panic apply and worklist integration (fourteenth follow-up cycle)

`cordial quality --apply --etiquette panics --dry-run` now collects fresh
evidence through the normal loader, enrichers, probes, assessors, and central
exception application. Apply retains crate-local finding anchors alongside
the shared workspace graph; it does not reconstruct analysis from source or
from rendered reports. The shared function inventory includes each unwrap's
line so an assessed site can be joined to its exact located operation. Missing
or ambiguous evidence refuses a candidate.

The initial supported operation is one resolved standard-I/O unwrap, through
its complete private single-module caller component to the actual binary unit
main. The configured error field strategy must be owned. The preview names
the generated parent, Kind, and source types, all changed functions, and the
intentional failure-policy change: panic becomes a returned error observed
through Rust Result main termination (Debug output and exit 1). This first
adapter does not implement miette rendering.

Removing `--dry-run` invokes the guarded shared writer. Only successfully
repaired sites are eligible for worklist claims; rule, crate, path, line,
context, and exact unwrap snippet must match. Unsupported expect/unwrap_err
and other panic sites remain manual items. Dry runs leave source and worklist
bytes unchanged. Missing or outdated worklists are not created or claimed.
Existing exceptions are respected before any migration is planned.

This integrates one complete operation rather than exposing partial signature
edits. Multiple ready plans in a crate are refused until their edits can be
composed against one snapshot. The next substantial coverage work is joint
construction for multiple failure sources, followed by wider module/call
identity resolution. Reverse transitions and alternate diagnostics remain
separate operations to test in isolation before composition.

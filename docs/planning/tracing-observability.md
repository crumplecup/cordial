# Automated tracing as a sensible observability default

**Status:** Phase 1 complete; Phase 2 in progress, 2026-10-31.

## Purpose

Cordial should give an ordinary Rust program useful observability by default.
An agent should be able to run tracing apply, inspect a comprehensible change,
and obtain useful execution context and error reports without designing every
annotation by hand. Generated tracing must compile, preserve program behavior,
and respect intentional instrumentation already present.

Success is an observable operation: what ran, with which safe identifying
context, where it failed, and whether that failure was handled or escaped.
Attribute counts alone do not establish this.

Keep comprehensive spans, including private helpers. Subscriber filtering
controls their visibility. Choose event placement and severity separately:
spans supply execution context; warning events should describe something worth
attention. Propagating an error through six functions should not automatically
produce six indistinguishable warnings.

Apply must preserve existing instrumentation through minimal edits. Compiled
`cordial explain tracing` remains the authority for currently shipped behavior
and must change alongside each implemented policy change.

## Starting evidence

The October 31 dogfood run exposed a missing-evidence defect: tracing apply
omitted `AttributeEnricher`, while assessment converted missing instrument
evidence into default arguments. Apply then rewrote 376 files while checking
off two worklist items. Phase 1 addresses this with the enrichment fix,
an assessment error for missing evidence, and regression tests. Its validation
checkpoint is recorded below; this does not certify the later policy phases.

### Weak spots and closure criteria

| Weak spot | Current evidence | How we close it |
| --- | --- | --- |
| Missing evidence becomes compliance or invented repairs | `assessor.rs` previously defaulted absent `PresentInstrument`; recipe strings still have defaults | Typed, required evidence with distinct resolved, unsupported, and unavailable states; fail malformed snapshots and report analysis gaps |
| Cross-file errors lose `err` recommendations | `display_types.rs` resolves aliases and Display implementations within one file | Resolve Result identities, aliases, error types, and formatting capabilities through canonical collection into shared IR |
| Type spelling substitutes for identity | `*Result` suffix checks and a displayable name list containing bare `Error` | Use resolved type and trait identities; unrelated names must not confer capabilities |
| Borrowed returns suppress errors | `fallible_err` rejects any return type containing a borrow | Evaluate the error payload's recording capabilities independently of the success value's ownership |
| Trait methods have a blanket exemption from `err` | `trait_surface_recipe` always sets no error event | Use the actual error contract and reporting responsibility; trait role determines span defaults, not universal error silence |
| Error severity disappears | `PresentInstrument` stores `err` as a boolean | Preserve default and explicit event levels, formatting mode, and cfg applicability; assess actual severity |
| A warning anywhere can count as error coverage | Body and boundary collectors use coarse presence booleans | Record which error/path an event covers, whether it executes, and which resolved helper reports it |
| Configuration alternatives can share evidence | Confirmed: a compliant cfg sibling hid the other declaration's missing error event | Declaration multimap and file/line joins now isolate siblings; keep adversarial regressions |
| One repair discards unrelated instrumentation | Whole-attribute replacement and text-based removal were unsafe | Located argument edits and attribute removal now consume shared IR ranges; unsupported forms remain unresolved |
| Recipes use names and size as behavioral proxies | I/O prefixes choose info; line count promotes Other to info | Prefer known operation roles and effects; use syntax heuristics only as explicit fallbacks, never as semantic proof |
| Payload selection is coarse | Name-based skip list and recordability heuristics | Separate formatting capability, payload size, and recording policy; select useful safe identity fields without dumping objects |
| Compliance does not prove runtime usefulness | Most checks inspect annotations or reports | Capture runtime spans/events under subscriber filters and assert context, severity, failure coverage, and duplication |

The cfg identity defect was reproduced and corrected during Phase 1. The
cross-file alias limitation remains confirmed in the current implementation.

## Invariants

- One canonical parse owner and one shared IR. Add needed facts during shared
  collection/enrichment. Consumers use `IrView`; no private source scan, type
  cache, secondary parser, or direct semantic backend access from apply.
- Facts and edits belong to a source revision, declaration, and configuration.
  A same-named function or a different cfg branch cannot supply another's
  annotation, error contract, or event coverage.
- Unknown evidence is different from a known absence. A missing trait answer
  must not mean either “safe to generate” or “no observability gap.”
- Every edit has an explicit finding or policy delta. Already compliant code
  is byte-for-byte unchanged. A second apply is a no-op.
- Preserve existing verifier gating, proof-only attenuation, and skip-policy
  behavior. Fixing error reporting must not reintroduce verifier-hostile spans.
- Keep user configuration in `cordial.toml`. No new environment-only policy
  channel, temporary adoption feature, or parallel tracing implementation.
- Preserve existing exception approval rules. Ordinary policy configuration
  is not permission to silently create exceptions.

## Target policy

### Spans describe execution

| Function responsibility | Default span | Recording |
| --- | --- | --- |
| Accessor, predicate, small setter, routine trait plumbing | trace | Small safe identity values; success recording only when useful and cheap |
| Internal computation, scan, render, ordinary construction | debug | Operation identity and bounded context; skip large or opaque values |
| User-facing operation, command, job, externally meaningful I/O boundary | info | Stable operation identity and useful outcome context |

Visibility does not exempt a function. A long function is not automatically
a user-facing operation, and an I/O-like name is not proof of an I/O effect.
Use canonical semantic evidence where available and describe fallback choices.
Retain existing finer-grained spans when compatible with the policy rather
than normalizing every annotation to an exact generated string.

Do not equate `Debug` with safe or useful to log. Keep a conservative default
for opaque, large, or potentially sensitive payloads. Preserve explicit user
fields and redaction choices. Never introduce `ret` merely because a value
can technically be formatted. Copyable does not imply nonsensitive.

### Events describe outcomes

Recommended default:

- A failed operation that escapes an identified reporting boundary emits a
  warning. A failed process/command boundary may use error severity. Preserve
  a deliberate stronger severity when it satisfies the contract.
- Pure propagation contributes spans and, where useful, diagnostic error
  events at debug. Avoid requiring a warning at every propagation layer.
- A handled failure that degrades service or exhausts retries merits a
  warning at the handler. Expected absence or ordinary control flow does not
  become a warning solely because it uses `Result`.
- Existing explicit events satisfy coverage only for the error paths they
  actually report. An unrelated warning, a warning in a nested function, or
  a call to a same-named reporting helper is insufficient.
- Do not infer “expected failure” from error names. Use explicit policy or
  trustworthy control-flow/contract evidence; otherwise retain uncertainty.
- Open library callers, dynamic dispatch, macros, and unavailable contracts
  can prevent proof of one reporting owner. Record that gap. Preserve current
  reporting, and use a conservative warning fallback for confirmed fallible,
  recordable functions whose reporting responsibility is unresolved. Do not
  claim deduplication or remove warnings on incomplete evidence.

Initial boundary evidence should cover binary/command entry points and
resolved configured reporting helpers. Extend to library operation boundaries
through explicit TOML policy and resolved identities. Public visibility alone
does not establish reporting ownership. Configuration must be able to override
span and error-event defaults by a well-defined scope, with documented
precedence and a reason shown on findings.

The exact configuration schema is an implementation deliverable. It must
express span levels, error reporting responsibility/severity, and recording
choices independently. Preserve existing `[tracing]` configuration. Test the
defaults as seriously as explicit overrides.

Configuration precedence is function, module, crate, then global defaults.
Make role-based span levels, responsibility-based error levels, boundary
selection, argument recording, and success recording tunable. If recording
size limits are introduced, expose their values with documented units. Avoid
line-count thresholds for event severity. Required evidence, revision/cfg
isolation, preservation of unrelated annotations, and idempotence are fixed
correctness requirements rather than configurable relaxations.

Warning deduplication requires proof along paths, including handlers and call
cycles. Do not treat a call graph as control-flow coverage or promise exactly
one event for an arbitrary program. Begin with closed, direct specimens and
expand supported cases while keeping unresolved cases visible.

### Recording capability is separate from policy

For each return contract retain the success type, error type, instantiated
generic constraints, and known formatting capabilities. A borrowed success
value does not prevent recording a displayable error. A formatting trait method
may need special recursion protection; ordinary fallible trait methods do not
inherit a blanket exemption.

Prefer Display for established error types. Consider Debug recording only
when the capability and payload policy explicitly permit it. An unresolved
formatting capability produces a visible apply blocker, with a reason, rather
than an annotation that might not compile or a false clean result.

## Shared IR evidence

Introduce or extend typed facts, reusing existing call, source, attribute, and
error-family facts rather than building a tracing-only copy of the program:

1. **Declaration:** canonical identity, source range/digest, module, target,
   enclosing cfg conditions, macro origin, and proof/toolchain policy.
2. **Contract:** resolved Result/Option identity and aliases, success/error
   types, trait bounds, and formatting capability with evidence/gap reasons.
3. **Instrumentation:** exact attribute and argument ranges, qualified macro
   identity, span level, skip/skip_all, fields, err/ret modes and levels, cfg
   guards, and unrecognized syntax retained for preservation or blocking.
4. **Execution and errors:** resolved calls and handlers, propagation edges,
   event identity/level/fields, associated error value and path, reporting
   helper contracts, and unresolved control-flow/dispatch boundaries.
5. **Decision:** selected span recipe, error reporting responsibility, policy
   source, evidence dependencies, precise deltas, and automation blockers.

Missing structural evidence should fail assessment. Valid but incomplete
language analysis should yield an actionable analysis-gap finding. Distinguish
those from normal policy findings and show them in human/JSON output.

Use the canonical backend to collect semantic evidence into the shared IR.
Extend its adapters and projections where required. Retire the superseded
per-file Display/name inference as semantic coverage replaces it. Keep
unsupported cases explicit during that transition.

## Apply contract

Assess and apply consume one typed decision and delta model. Reporters render
that decision; apply does not rediscover it from report text or source scans.

- Insert an annotation only when missing. For existing annotations, change
  only the arguments required by the approved policy delta.
- Preserve `name`, `target`, `parent`, custom fields/expressions, `skip_all`,
  `ret`, formatting modes, paths/import scope, comments, and cfg wrappers
  unless one is the subject of that delta. Keep stronger compatible existing
  choices. Unsupported syntax blocks the affected edit instead of triggering
  full replacement.
- Deduplicate findings that demand the same edit. Missing `err` and silent
  error coverage must not cause conflicting writes or duplicate repair work.
- Check all ranges and source digests before writing. Use the existing source
  transaction for related edits; never leave a partial repair after failure.
- Do not inject subscriber installation into a library caller or invent
  missing error handlers as a side effect of an attribute repair. Leave such
  cases open with concrete guidance until their transforms are supported.
- Dry-run reports candidates, files, exact edits, and blockers without source
  or worklist writes. Real apply checks off only matching items it actually
  repairs, reports closed/remaining counts, and leaves submit to verify them.
  Existing suppressed findings remain suppressed and untouched.

## Implementation sequence

Each phase follows implement, focused validation, build-tier submit, and a
scoped commit. Save the all-tier gate for completion of this plan. Use the
default store and ordinary installed binary; reinstall when changed behavior
is needed for dogfood.

### 1. Restore trust in evidence and edits

- [x] Finish and gate the pending missing-AttributeEnricher fix and regressions;
  clear the new test subscriber finding through the normal workflow.
- [x] Reproduce cfg alternatives and same-named declaration collisions, then
  fix their shared IR identities/joins with adversarial tests.
- [x] Replace destructive attribute replacement with minimal delta edits.
- [x] Preserve event levels/modes and all existing attribute choices in typed
  facts; reject missing required evidence and unsupported edits explicitly.
- [x] Prove a one-argument repair preserves every unrelated argument, then
  prove a second apply makes no changes.

Implementation checkpoint: attribute facts now include exact complete-attribute,
meta, argument, and value ranges from the shared parse, including nested list
entries and enclosing gate predicates. Apply merges missing fields/skips/err
without replacing explicit return recording, error mode/severity, custom fields,
comments, or qualification. Removal uses the complete owned attribute range;
mixed cfg-attribute payloads and ambiguous declarations remain unresolved.
Uniform CRLF and missing final newlines are preserved; mixed line endings are
explicitly blocked. Proposed syntax and source snapshots are validated before
the existing multi-file source transaction writes anything.

Existing finer span levels remain intact. Unknown level expressions, multiple
instrument annotations, mismatched verifier gates, and short-form annotations
requiring a new verifier gate remain unresolved. The last case needs shared IR
import ownership before apply can safely avoid unused imports in prover builds.
Argument syntax is now typed and preserved; severity/mode policy comparison
still belongs to Phase 3. New annotation insertion retains its existing located
line helper; this checkpoint does not claim complete semantic edit planning.

File-local planning is separated from the transaction coordinator. Dry-run
retains validated proposals separately from written edits, so its proposed
worklist-closure count is accurate without checking off any items. Regression
coverage checks both the proposed count and unchanged source/worklist data.
The focused apply and evidence suites pass 41 and 2 tests respectively; the
22 tracing assessment tests, 24 crate-attribute tests, and canonical/compatibility
backend projection parity also passed during this phase. A slim `crate_attrs`
build ran its 24 tests without warnings.

Dogfood repaired 13 tracing items across seven files and checked off their
matching worklist items. The build-tier submit then verified all 16 Phase 1
repair claims (including field visibility and apply organization), with no new
findings. The subscriber-init claim had already cleared in the preceding submit.

### 2. Close error-contract blind spots

- [ ] Resolve cross-file aliases, imports, generic bounds, and formatting
  capabilities through the canonical backend into shared IR.
- [ ] Remove the borrowed-return veto and blanket trait-method exemption,
  replacing them with capability and recursion-aware decisions.
- [ ] Test custom non-Result types ending in Result and unrelated Error types;
  neither may acquire capabilities through its name.
- [ ] Add explicit gap findings and blockers; unknown is never silently clean.
- [ ] Compile specimens for borrowed success/error values, local and imported
  aliases, generic errors, trait implementations, and non-Display errors.

Collection checkpoint: canonical function projections now retain independent
Display and Debug obligations for the actual Result error, evaluated in the
function's generic environment. These facts cover cross-file aliases, borrowed
success/error values, conditional generic implementations, and trait methods.
Formatting roles distinguish core Display/Debug methods from ordinary methods
and unresolved implementations. Missing evidence stays unavailable, including
when older snapshots lack the new fields. An unresolved success type does not
erase an established error capability.

These facts are stored on shared IR function nodes and participate in tracked
query invalidation. The production tracing inventory still uses its file-local
collector. Next, route canonical projection through the shared loader into that
inventory, preserving declaration/configuration identity; then replace the old
name inference and expose unresolved capabilities as findings/apply blockers.
Do not load a second semantic database from a tracing hook or apply.

### 3. Establish observability policy and explain it

- [ ] Implement independent span, recording, and error-event policy in TOML,
  including defaults, precedence, resolved boundary selection, and overrides.
- [ ] Refine role defaults using shared operation/effect facts. Stop promoting
  spans solely because a function is long.
- [ ] Compare actual error-event severity/mode rather than boolean presence.
- [ ] Specify safe argument/return recording and preserve existing redaction.
- [ ] Update compiled explain, config examples, rule meanings, and migration
  guidance together. Findings state what is needed, why, and whether apply
  can do it.

### 4. Attribute reporting to real error paths

- [ ] Replace warning-presence booleans and helper-name matching with shared
  resolved event/path/helper evidence.
- [ ] Support direct propagation chains and handlers first, then branching,
  shared callees, and recursive components with explicit coverage limits.
- [ ] Establish boundary reporting before reducing proven duplicate warnings.
  Preserve warnings and show gaps where ownership cannot be established.
- [ ] Keep boundary, err-missing, and silent-path assessment consistent; do not
  emit multiple repair tasks for one missing error report.

### 5. Prove useful defaults on executable programs

- [ ] Add small executable specimens with captured subscribers. Start with
  success/failure and one caller; build up to several callers, recovery,
  retries, and boundary delegation.
- [ ] Assert emitted severity, event count, useful fields, and span ancestry
  under info/debug/trace filters. A debug span with a warning event needs an
  explicit filtering test so required context does not disappear unnoticed.
- [ ] Assert values, source chains, caller locations, compilation, and
  intentional side effects are unchanged by apply. Test async span behavior
  and explicit-parent/target preservation where supported.
- [ ] Include unrelated warnings, nested closures/functions, cfg alternatives,
  recursive formatting, and unresolved dispatch as negative cases.
- [ ] Dogfood: dry-run, inspect proposed edits, apply, focused checks, submit.
  No mass rollback and no manual reconstruction of mechanical apply output.
- [ ] Measure collection, enrichment, assessment, edit planning, and emitted
  event volume using the existing hotpath/progress infrastructure. Reuse IR
  evidence across consumers; do not add a parallel timing store.
- [ ] Run the end-of-plan all-tier gate, clear every open item, and record the
  verified coverage and remaining explicitly blocked patterns.

## Completion

The plan is complete when supported programs get comprehensive context,
correctly placed and leveled error events, safe useful fields, and repeatable
minimal apply edits; unsupported evidence remains visible and cannot yield a
false clean result. Runtime tests, compilation tests, worklist verification,
and the final all-tier gate must agree.

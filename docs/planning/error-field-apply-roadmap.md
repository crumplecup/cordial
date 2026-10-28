# Error field `--apply` roadmap

**Status:** Active. Work in small, independently gated slices. This plan covers
`cordial quality --apply --etiquette internal_error_chain` under the
`[internal_error_chain] field_strategy` switch. It is a roadmap, not a claim
that every borrowed error can be generated automatically.

## Goal and current boundary

Switching between `owned` and `borrowed` should produce useful, reviewable
source edits wherever Cordial can prove the conversion. The user should see
which fields were converted and why the others were left alone. Both directions
matter: `borrowed` is useful to verifiers, while `owned` must let a project
reverse a prior migration.

This is a strategy migration, not a collection of local lint fixes. A policy
may permit several mutually exclusive code designs; `--apply` can move a
complete conversion unit between them when the necessary evidence proves that
the code's purpose and behavior are preserved. The same model can later cover
other policy choices, such as test error handling with `miette`, `anyhow`, or a
boxed error. The chosen strategy controls the transformation; the collected
facts and unresolved edges remain independent of that choice.

The apply pass changes private error struct fields with
recognized constructors. It handles `file: String` built from
`Location::caller().file()` and other `String` fields built from string literals,
or explicitly qualified same-module `const` and immutable `static` bindings
typed `&'static str`, including constants whose values use `concat!(...)`.
Direct `::core` or `::std` forms of `concat!` and `include_str!` also qualify.
Unique block-scoped `const` and immutable `static` bindings explicitly typed
`&'static str` qualify when no local name can shadow them.
An explicit `&'static str` constructor parameter used through a recognized
owned-string conversion qualifies when the body has no same-name binding that
could shadow it.
An explicitly typed, immutable top-level local `&'static str` alias also
qualifies after its declaration when no same-name binding occurs in the body.
It recognizes `to_string`, `to_owned`, `into`, and selected `String::from` forms.
It mirrors those conversions back to owned `String`. A private error type with
one existing named lifetime and a separate non-string lifetime-bearing field
can also borrow constructor parameters and typed local aliases tied to that
same lifetime. A closed private error with one string field and optional scalar
context can move an explicit constructor lifetime onto its type and back. It
can also introduce and remove a named lifetime across a closed private parent,
Kind, and native source graph spanning files. The cross-file planner requires
one constructor-anchored lifetime and proven borrowed origins at changed
constructors. It uses shared parsed files and validates patched source. It
leaves unrelated dynamic constructor parameters, unresolved graph callers,
public types, and ambiguous cross-file uses untouched. See
[`apply.rs`](../../src/etiquettes/internal_error_chain/apply.rs) and its
[`initializers.rs`](../../src/etiquettes/internal_error_chain/apply/initializers.rs).

Parsing is weaker than type checking: a converted field might have a read site
that still requires `String`, or a same-name constructor might have escaped the
scanner. Broader rewrites need a stronger evidence boundary before they edit.

## Safety contract for every slice

1. Build one conversion unit containing the field, all its construction paths,
   and the uses that depend on its type. If any path is unknown, leave that unit
   unchanged and report a specific blocker. Keep other independent units usable.
2. Resolve the intended type and expression, including imports, aliases, and
   shadowing, before claiming a conversion. A syntactic lookalike alone is not
   proof that an expression yields a `&'static str` or that a field access is
   read only. Start with explicit forms where resolution is tractable.
3. Never invent a `'static` lifetime, leak storage, or replace a dynamic value
   with a constant. Borrowed mode may require a named lifetime through the
   whole error chain; if that cannot be proved and propagated, explain the
   blocker and leave the code alone.
4. Preserve the location returned by `Location::caller()`, the typed `source`
   chain, error messages, and public behavior. Follow `Error::source()`'s
   lifetime constraints when changing source storage.
5. Dry run and write mode use the same edit plan. Apply a multi-file unit only
   when every edit is ready; avoid partially migrated types. Re-running apply
   under one strategy must make no further edits. A round trip should compile
   and preserve behavior, although source formatting need not be byte-for-byte
   identical.
6. Use `SharedParsedFile` for outer Rust files. Reuse the loaded trees and
   source provenance; do not add an independent parse for each pattern or
   etiquette. Track analysis time and changed-file counts as coverage grows.

## Ordered slices

| Slice | Candidate pattern and edit | Evidence needed | Done when |
| --- | --- | --- | --- |
| 0. Guard today's rewrites | Audit field reads and writes before changing a field. A `Display` read can usually remain; `self.message.push_str(...)`, an owned return, or a `String`-only call needs a proven companion edit or a blocker. Reject escaped constructors and unresolved names. | Same-file use inventory and explicit resolution for the current narrow candidates. | Compile fixtures cover harmless reads, incompatible uses, shadowing, missing constructors, dry run, idempotence, and both strategies. No current supported case regresses. |
| 1. More static string expressions | Recognize proven `&'static str` sources such as a local `const`/`static` and selected built-in expressions such as `concat!(...)` or `include_str!(...)`, with the existing owned constructors around them. Reverse to an owned conversion when selected. | Resolve the binding or macro, its result type, and all initializers. Do not treat an arbitrary function returning `&str` as static. | Each added form round trips and compiles; same-spelling local names and custom macros remain untouched. |
| 2. Constructor parameters and local aliases | Convert a private error field built from a `&'static str` parameter or a straight-line alias of one. For a dynamic `&'a str` parameter, propagate `'a` through the private error type only when all constructors and uses can be updated. Mirror the owned direction with an explicit allocation. | Parameter type, alias origin, constructor set, every call site, and escape/lifetime bounds. | Small same-file fixtures compile both ways; ambiguous aliases, mixed origins, and unknown callers report blockers without edits. |
| 3. Private Kind and source graph | Migrate `Box<Kind>`/owned Kind payloads and native `source` fields to references, plus constructor and wrapper expressions, when the whole private type graph admits one coherent lifetime. Reverse the graph to owned storage when requested. | Complete graph of parent, Kind, sources, constructors, trait implementations, and all callers. Check `Error::source()` and `From`/`?` constraints explicitly. | Multi-type fixture compiles with both policies, preserves source traversal and caller location, and is written as one conversion unit. |
| 4. Cross-file private chains | Apply the same graph conversion when declarations and constructors live in different modules or files. | Module/name resolution, visibility, complete call graph, and all files in the unit. | A multi-module fixture proves no partial write when one constructor or use is unresolved. |
| 5. Wider API and dynamic payloads | Investigate public error types and runtime `String`/`format!` values only after private conversions are reliable. These may need caller-visible signature changes or borrowing from longer-lived input. | Explicit API-impact decision and lifetime proof; no automatic conversion solely from a syntactic match. | Document supported subcases and blockers first; automate only subcases with compile and behavior evidence. |

Slices 0–2 are the near-term work. Slices 3–5 may expose a complexity wall;
the measured blockers should determine whether to split them further. A
runtime-created string that cannot outlive the error is a valid reason to
leave a finding for manual design, not a reason to force an apply rewrite.

## Reporting and validation

- Apply now reports converted fields, exact compliance sites, and named blockers
  on stdout. After successful writes, it checks off only matching field-storage
  and Kind-storage findings in the current worklist. Matching includes crate,
  file, original line, qualified context, and rule; unrelated findings on the
  same type or line stay open. `cordial submit` verifies the claims. Missing or
  invalid worklists are reported and left unchanged.
- Dry run uses the same plan and reports proposed worklist closures and the
  remaining manual items, without changing source or worklist state. Repair
  locations come from declaration facts collected into the shared IR, rather
  than a source scan in the reporting path.
- Remaining reporting work: expose exact planned edits and machine-readable
  candidate evidence, so an agent can inspect a complete conversion plan
  without parsing tracing logs.
- For each new form, test owned → borrowed → owned, dry run, repeat apply,
  unsupported lookalikes, and a compiling fixture that exercises the changed
  constructors and uses. Add behavior checks for source traversal and caller
  location when their graph is involved. A parse-only test cannot certify a
  type-changing rewrite.
- After focused checks, use `cordial open` and
  `cordial submit --tier build --deny-open` with zero open items before each
  phase commit. Save the expensive all-tier gate for the end of the roadmap,
  consistent with the current tier cadence.

### Coverage from real projects

Exercise a strategy change in an isolated copy, using that copy's default store:

```sh
cordial --project /path/to/project quality --apply --etiquette internal_error_chain --dry-run
```

Start with Amenable in `~/repos/amenable_copy`, with an independent Git repository
and a tested owned baseline. Switch `[internal_error_chain] field_strategy` to
`"borrowed"`, preview and apply supported repairs, and compile/test the changed
crates and their consumers. Commit benchmark transforms so a failed rewrite can
be preserved and reverted to a known checkpoint. Switch back to `"owned"` and
exercise the reverse direction on the successfully converted subset; full
forward coverage is not required for a partial round trip. Keep the original
project untouched and periodically clean accumulated build artifacts in the copy.

Record the strategy, proposed repairs, blockers, missed candidates, validation,
and checkpoint commits before selecting the next pattern. A no-op scan under the
project's existing policy does not measure migration coverage. Reduce a real
blocked case to a compiling regression fixture, then add its missing evidence to collection and
enrichment in the shared IR. Keep expected limitations (public API changes,
runtime-owned values, unresolved macro or lifetime edges) distinct from missing
support for a provable private conversion. Counts of blocked candidate fields
are not counts of all unsupported error-handling patterns: apply's inventory
still defines which candidates it can see.

#### First strategy-flip benchmark (2026-10-28)

The independent Amenable snapshot includes its original working tree changes
at HEAD `32b5960`. Copy baseline `a071ad0` (tag `baseline-owned`) passed
`cargo test --workspace --locked`, including compile-fail tests and ordinary
Rust doctests. Borrowed configuration checkpoint: `1bc987a`. Cordial: `fd349cc`.
Detailed benchmark notes are tracked in the copy at commit `5b0b527` in
`docs/error-strategy-benchmark.md`; reports stay in its default store.

The borrowed dry run and write pass both found **0 convertible fields** and
**155 blocked candidate fields**. The write pass changed no source, checked off
0 items, and left all **336 error handling items open**. No converted subset
exists to reverse yet; this is not round-trip evidence.

| Borrowed field-strategy finding | Count | Apply inventory |
| --- | ---: | --- |
| Call-site `file: String` | 118 | Blocked |
| Other `String` fields | 37 | Blocked |
| Kind tuple payloads | 113 | Absent |
| Other non-Copy fields | 62 | Absent |
| Total | 330 | 155 represented; 175 absent |

The comparison joins findings and blockers by declaration file, type name, and
field name. Five additional parent Kind storage findings and one caller-tracking
finding account for the remaining six items. The 155 blockers comprise 143
`OutsideFile`, 10 `FieldUse`, and 2 `Initializer` results. These coarse reasons
combine distinct causes: `OutsideFile` includes public containing types even
when the candidate field is private; `FieldUse` includes unresolved derives and
attributes, such as Amenable's `derive_more` errors, as well as actual field uses.

Next implementation targets, grounded in this sweep:

- [ ] Expose missing storage repairs and distinguish blocker causes from the
  existing shared-IR evidence, including public visibility, external references,
  generated code, and missing borrowing origins.
- [ ] Support safe call-site file conversion on public containing types and
  derive-based errors while preserving private field use contracts and rejecting
  unknown generated accessors or constructors. This targets 118 real fields
  without adding lifetimes to their enclosing types.
- [ ] Run both strategy directions on that converted subset, checking compilation,
  behavior, source traversal, caller location, worklist claims, and repeat apply.
- [ ] Extend caller-owned lifetime origins and connected source/Kind migrations
  from the remaining cases. Never manufacture references to constructor-local
  temporaries, leak allocations, or weaken source-chain behavior to force a plan.

## First implementation decision

Start with slice 0. It sets the minimum evidence needed for the existing
rewrites and gives later slices a reusable way to decline unsafe changes.
After that, choose the first slice 1 expression based on actual unresolved
cases in a fixture or dogfood project, rather than adding syntax forms simply
because they are easy to recognize.

### Slice 0 progress

The first guard declines a conversion when the field is used outside a simple
formatting argument, destructured from the error, or constructed inside a
macro the initializer visitor cannot inspect. Regression fixtures cover
mutation, owned return, struct pattern, and macro construction; existing
formatting reads still round trip.

The next guard audits each parsed file once for visible imports, aliases,
glob imports, and local definitions that could shadow `String`, `Location`,
`write!`, or the error type. It declines a field when an error type has an
alias that could hide another constructor. The apply summary now exposes the
candidate file, type, field, and blocker reason; the apply pass also emits
these as tracing events in write and dry-run mode. Compiling fixtures cover
aliases, a custom `String`, a custom `Location`, and a custom formatting macro.
These are conservative source checks, not complete Rust name resolution:
macro expansion and names introduced outside the file still need a stronger
evidence boundary. Slice 0 remains open for those cases.

### Slice 1 progress

Explicit `self::NAME` references to a unique, unconditionally declared
same-module `const` or immutable `static` typed `&'static str` now round trip
between owned and borrowed storage. The binding's type supplies the proof, so
the value may use `concat!(...)` without interpreting macro expansion. Direct
`::core` and `::std` calls to the built-in `concat!` and `include_str!` macros
also round trip. Bare module names, conditional or duplicate declarations, and direct
macro expressions without a qualified built-in path remain unsupported until
their bindings or expansions can be resolved safely.
Unique block-scoped `const` and immutable `static` declarations typed
`&'static str` now work through bare names within the enclosing constructor;
same-name local bindings and opaque local imports still block that conversion.

### Slice 2 progress

The first constructor-parameter case keeps the constructor signature intact:
`String::from(parameter)`, `parameter.to_string()`, `parameter.to_owned()`, and
`parameter.into()` become `parameter` when it is explicitly `&'static str` and
no binding in the body shadows it. Owned mode restores `String::from(parameter)`.
The same round trip works for an immutable
top-level local alias explicitly typed `&'static str` when it is in scope at
the construction site and has no same-name binding elsewhere in the body.
Shadowed and untyped aliases remain blocked. Elided `&str` parameters, wider
lifetime propagation, and constructor-call edits are still future work.

For a private error struct that already has exactly one named lifetime and a
non-string reference field that keeps it in use under both strategies, a
matching `impl<'a> MessageError<'a>` can convert a `String` field to `&'a str` when
every recognized constructor uses `&'a str` parameters or typed aliases.
The reverse conversion restores owned storage. A parameter with an unrelated
or elided lifetime blocks the field. A shorthand borrowed initializer such as
`Self { message }` now expands to `message: String::from(message)` in owned mode;
the borrowed direction recognizes the resulting explicit initializer. This
slice does not add lifetimes or edit caller signatures.

### Conversion-unit inventory

Adding a lifetime to an error type, or borrowing its `Kind` and `source`, is
now supported by a conversion-unit inventory built from the shared parsed files.
Each candidate field carries located evidence for direct constructors, associated
calls, type uses, field reads and patterns, `From` implementations, `?` sites,
`source`/`kind` edges, aliases, macros, and cross-file references. A derive or
attribute that may generate a constructor is recorded as unresolved. The
single-file apply path uses the inventory to reject unknown field, macro, alias,
generated-constructor, and cross-file paths. Existing narrow conversions remain
available when those paths are absent.

The inventory is syntactic: a direct path has a known target shape, while macro
expansion, type inference for field receivers, and `?` conversion targets remain
unresolved. Broader conversion planners must require a closed unit, plan every
field, constructor, caller, signature, and error edge together, validate the
whole unit, and stage all of its files before writing them with rollback on a
later replacement failure before extending slices 2 and 3.

The first lifetime planner handles a closed, private error with one migrated
string field and optional scalar `Copy` context fields, whose
one constructor has an explicit named lifetime on its `&str` parameter. The
constructor can take matching scalar context parameters. It
moves that lifetime from the constructor method to the error type and its
`Display`/`Error` impls, and reverses the move under owned strategy. Direct
associated constructor calls may keep type inference. A private function with
the same named input lifetime can forward that input directly to the one
constructor; matching scalar parameters can pass through unchanged. Its
explicit return type changes with the error type in both directions. Other
explicit type uses still block the edit until their containing
signatures and callers can be planned together. Compile-checked fixtures cover
the round trip, dry run, repeat apply, and that blocker. Additional owned or
borrowed fields, multiple constructors or files, and typed source/Kind edges
remain outside this planner.

### Private Kind and source graph

The writer plans every file in a crate target and checks all planned source
snapshots against disk before writing. Multi-file writes are still not atomic.
For a file-local private error with an existing named lifetime, apply
plans the parent, its Kind variants, nested native source types, constructors,
storage-sensitive reads, and `Error::source()` as one edit set. It converts
owned and borrowed storage in both directions. If a required constructor or
use is unrecognized, it leaves the entire file-local graph unchanged and
reports a graph-closure blocker. A compiling round-trip fixture exercises
`From`/`?`, source traversal, Kind access, and caller location. A closed
file-local graph can also acquire one constructor-anchored lifetime across its
declarations, impls, and typed uses, then remove it on the owned round trip.
A return that would borrow local values blocks the entire edit. A compiling
round-trip fixture covers both directions and the blocker. A graph with an
existing named lifetime can now convert a constructor in a different external
module file. The planner resolves each graph type name to its declaration
identity from the shared parsed tree, plans constructor edits in every
relevant file, and requires construction coverage across the combined edit
set. An external field read, shadowed declaration, hidden macro use, or
unknown path blocks the whole graph. The patch pass withholds every file in
the component if final validation rejects one member. A compiling dry-run and
owned-to-borrowed-to-owned fixture exercises a parent error with a native
source and a constructor in a child module. A closed graph with an existing
named lifetime can now also include Kind and source declarations in different
files. The shared declaration index locates each member, and one component
contains its declaration, constructors, and source-sensitive reads. A four-file
fixture checks dry run, both storage strategies, compilation, and an atomic
blocker. The planner can now introduce that lifetime across four files and
remove it on the owned round trip. An unanchored constructor blocks the whole
graph. Inventory and graph planning share one crate type index during
apply. The inventory also records the
syntactic storage shape and type path of direct, boxed, and explicitly borrowed
`source`/`kind` fields as typed IR facts. Enrichment links a unique private
top-level struct or enum to its declaration identity and snapshot digest.
File-local references and explicit module paths or `use` aliases can link
across active external modules in the same crate target. The shared IR holds
dependency edges from conversion units to those declarations. These are
candidate links, not proof that every conversion graph is closed;
the supported cross-file conversion requires a separate whole-graph check.
Public, ambiguous, macro-attributed,
cfg-gated, glob-imported, and unrecognized module paths stay unresolved. The
same shared-tree visit records the written input and output type
paths of `From` impls. Each `?` site also records its lexical function,
closure, async block, or try block scope and any written return and `Result`
error paths. Explicit closures with a different return error are kept separate
from their outer function; inferred nested scopes remain unresolved candidates.
The written paths remain unresolved, as do the operand error and conversion
target types of `?` expressions. For a
`From<io::Error>` path, changing `source: io::Error` to a reference changes
who owns the `io::Error`; the syntax of `?` alone cannot prove the caller can
hold it long enough. The unresolved-constructor fixture checks that the entire
graph stays unchanged.

The same inventory walk records the written return type and direct `self` field
uses of each `Error::source()` override. These are syntax facts, not proof that
the method returns that field on every path or that the trait name resolves to
`std::error::Error`; the conversion edge remains unresolved.

### Pipeline contract for broader apply

`quality --apply` now builds the same `SourceLoadView` as normal sessions and
reads conversion units from the error IR. Its site collector identifies
  candidates first, then visits each shared syntax tree once per file to attach
construction and use sites. The special lifetime collector indexes top-level
items once per relevant file, though it still visits candidate initializer
expressions separately. Apply validates source identity and edit ranges without
parsing the patched text; fixture tests compile the transformed code. Field
declarations, storage shape,
visibility, initializer choices, lifetime edits, field-use blockers, and
ambiguous-name decisions now travel in typed conversion-unit IR records. Apply
reads only source text and digests through `SourceEditView`. These records still
need to become a complete unit query. A missing fact belongs in collection or
enrichment, not in an apply-specific fallback.

The crate-wide declaration resolver, constructor lifetime anchors, incoming
Kind/source edges, cross-file identifier and path locations, cfg and macro
markers, and name ambiguity audit now live in the shared IR. The normal error
enricher and apply build these facts and the selected graph plans through the
same enrichment entry point. Graph planning reads the resolver back from IR;
its old per-candidate path, name, and incoming-edge visitors are gone. Graph
constructor, field-use, pattern, source-method, Kind, and macro sites now come
from strategy-independent expression facts in that IR. The graph expression
planner reads those facts and source text without walking the syntax tree.
Lifetime declarations, impls, methods, return safety, type uses, and unsupported
expression paths now come from typed IR facts too. Field lifetime requirements
and retained lifetimes are collected there, so graph lifetime selection and edit
planning no longer launch a syntax visitor for each candidate. Declaration and
field shapes, candidate fields, and cross-file mentions also come from the IR.
Graph planning receives `SourceEditView`, which has no syntax-tree access. The
older initializer and file-local lifetime collectors visit the shared tree
during collection. File-local lifetime syntax evidence enters the crate index
in the shared IR. Conversion-unit assembly reads that index back from `IrView`,
checks closure and type uses, and stores the selected ranges in each unit.
Apply constructs edits from those facts and the source snapshot.

| Stage | Facts or responsibility |
| --- | --- |
| Source load | Retain the exact text, shared syntax tree, file identity, and content digest for every in-scope Rust file. Collection and enrichment use the tree; apply receives text and digest for located edits. |
| File-local collection | Record module-qualified declarations and visibility; field names, types, lifetimes, and spans; enum variants; impls and derives; imports, aliases, bindings, shadowing, and cfg; constructors with each field's initializer and value origin; calls, explicit type uses, field reads/writes/patterns, return signatures, `From` impls, `?` sites, and `Error::source()` bodies. Mark macro-generated or otherwise hidden paths explicitly. Collect these once per file, rather than revisiting every file for every field. |
| Cross-file enrichment | Resolve references to declaration identities and connect type, constructor, caller, field-use, Kind, source, `From`, and `?` edges. Classify each value's provenance (`'static` literal/binding, borrowed parameter with a named lifetime, owned runtime value, foreign source, or unknown). Record the lifetime and ownership constraints imposed by each edge, including `Error::source()`'s trait-object lifetime. Preserve an explicit unresolved reason where name resolution, inference, cfg, or macro expansion prevents proof. |
| IR conversion-unit query | For each candidate, query the shared graph for the closed dependency component: every declaration, initializer, caller, signature, field use, and error edge affected by changing its storage. Include exact source ranges and snapshot digests. A unit is ready only when every required edge is resolved and each use is either unchanged by the new type or has a planned companion edit. No separate unit database. |
| Apply | Select the owned or borrowed transform against that view, calculate all edits in memory, reject conflicting or stale spans, validate the complete patched unit, and write all its files together. Dry run uses the same plan and reports edits and blockers. |

The first collection addition should cover the existing private string-field
patterns, so the old apply path and the new view can be compared on the same
fixtures. Next add explicit constructor and forwarding-signature edges for the
current lifetime planner. Only then add Kind variants and typed source edges
for the first graph fixture. The collector records syntax and provenance; the
enricher establishes relationships and their limits; the transform decides
whether a requested strategy is possible. Keep these facts independent of the
chosen strategy so an owned → borrowed → owned round trip can reuse the same
model after reloading changed files.

The serializable IR should hold compact, typed facts and stable declaration/site
identities, not duplicate source text or a second syntax tree. Give the planner
an `IrView` and a source-text-only edit view with no `syntax()` method; make
the richer `SharedParsedFile` available only to collection and enrichment.
This API boundary should make bypassing the graph harder than extending it.
A cached IR is usable for apply only when its source digests match the files
being edited; otherwise rebuild it. `syn` does not provide full Rust name or
type resolution, so an unknown edge is a blocker, not evidence that no edge
exists.

Validate this boundary with a fixture in which a second file adds a constructor
or type use: collection must find it, enrichment must attach it to the correct
type, and apply must either edit the entire unit or write nothing. Add an
ambiguous-name/macro fixture that remains blocked, a stale-snapshot fixture,
and a graph round trip that checks source traversal and caller location.

Implementation checkpoints:

- [x] Plan a private file-local Kind/source graph as one IR-backed edit set,
  with source-digest validation, conservative blockers, and a compiling
  owned-to-borrowed-to-owned fixture. A constructor-anchored lifetime can now
  be introduced across a closed same-file graph and across supported closed
  cross-file graphs. The multi-file writer stages edits with rollback.

- [x] Build apply's source view with the same loader as normal sessions, and
  materialize conversion units as typed graph facts in the error IR enricher.
  Apply now reads those units through `IrView` and fails if one is missing.
- [x] Collect conversion sites once per file. Candidate declarations are
  identified first; a single visitor per shared source tree dispatches located
  sites to every relevant field, then the units are materialized in the graph.
  The collector still checks candidate names against each file's text before
  walking it, and name-resolution work inside selected hooks remains a possible
  optimization target.
- [x] Move initializer, field-use, and lifetime-planning evidence into typed IR
  facts. The ordinary field planner reads declaration and initializer facts
  from each conversion unit and builds edits after reading the IR. The special
  lifetime path stores typed ranges for the field, struct, impls, forwarding
  returns, and constructor initializers; apply builds its edits from those facts.
  Syntax checks now produce lifetime site facts in the shared crate index; the
  unit pass reads that index from `IrView` to check closure and type uses. A
  per-file top-level index serves every lifetime candidate during collection.
  Graph lifetime facts already live in the shared IR, so graph lifetime
  planning does not walk the tree for each candidate.
  Apply now receives `SourceEditView`, which exposes text and digest but no
  `syntax()` method.
- [x] Put crate-wide declaration resolution, cross-file path and name checks,
  incoming graph edges, and constructor lifetime anchors into the shared IR.
  Both normal scans and apply run the same enrichment entry point. The graph
  planner consumes these facts from IR, while its expression and lifetime edit
  expression and lifetime planners read typed sites from IR. Declaration
  shapes and older initializer evidence still need the typed extraction above.
- [ ] Allow cross-file conversions only after the planner proves a closed unit.
  Shared source snapshots and conversion units carry digests. Apply now stages
  every planned file in its source directory, checks all snapshots before
  replacement, checks each again at replacement time, and restores earlier
  files when a later replacement fails. A two-file fixture exercises the write
  path. Add deterministic failure tests for rollback and concurrent changes.
  This is
  a rollback-capable whole-target write, not crash-atomic across files.
  Unknown name, cfg, or macro edges remain explicit blockers until the planner
  can account for them. A private parent error and native source declared in
  one file, with a constructor in another, now convert together when every
  external type name resolves uniquely. The patch pass treats those files as
  one component.
  Closed Kind/source declarations across active external modules now convert
  together when the parent already has a named lifetime; a four-file fixture
  checks both directions. Cross-file lifetime introduction now covers a closed
  four-file graph with a named constructor anchor and proven constructor
  origins; more complex caller constraints remain open.
- [x] Connect conversion evidence across validated external module boundaries
  in the shared IR. A private declaration has a path, line, column, and digest identity;
  source/Kind edges and path-based constructor, caller, and type-use sites link
  to it when module and import resolution are unambiguous. These links do not
  yet authorize cross-file conversion.

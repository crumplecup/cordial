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

The current apply pass changes private, same-file error struct fields with
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
same lifetime. A closed private single-field error can move an explicit
constructor lifetime onto its type and back. It uses a shared parsed file view
while scanning and checks that edited source parses. It leaves unrelated
dynamic constructor parameters, wider lifetime propagation, `source` and `kind`,
public types, and cross-file uses untouched. See
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

- Extend the apply summary from `converted_fields`/`unresolved` to explain
  candidate identity and blocker reason. Dry run should present the exact
  planned edits and blockers without writing. Keep machine-readable output in
  view as the summary grows, so an agent can act on blockers without parsing
  tracing logs.
- For each new form, test owned → borrowed → owned, dry run, repeat apply,
  unsupported lookalikes, and a compiling fixture that exercises the changed
  constructors and uses. Add behavior checks for source traversal and caller
  location when their graph is involved. A parse-only test cannot certify a
  type-changing rewrite.
- After focused checks, use `cordial open` and
  `cordial submit --tier build --deny-open` with zero open items before each
  phase commit. Save the expensive all-tier gate for the end of the roadmap,
  consistent with the current tier cadence.

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
whole unit, and write its files atomically before extending slices 2 and 3.

The first lifetime planner handles a closed, private, single-field error whose
one constructor has an explicit named lifetime on its `&str` parameter. It
moves that lifetime from the constructor method to the error type and its
`Display`/`Error` impls, and reverses the move under owned strategy. Direct
associated constructor calls may keep type inference. An explicit use of the
error type outside those impls blocks the edit until its containing signature
and callers can be planned together. Compile-checked fixtures cover the round
trip, dry run, repeat apply, and that blocker. Multiple fields, constructors,
files, and typed source/Kind edges remain outside this narrow planner.

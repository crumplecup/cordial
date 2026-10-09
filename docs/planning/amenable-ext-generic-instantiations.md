# Generic instantiations as coverage rows (amenable-ext)

Status: **Active**

Origin: downstream amenable report "Report generic type instantiations as
separate coverage rows". **The amenable repo owns the plan**
(`amenable/docs/CHRONO_SUPPORT_PLAN.md`, "Cordial requests"); this document
tracks cordial's side and adapts to it where they differ. Builds on
[amenable-ext-targets-config.md](amenable-ext-targets-config.md) and
[amenable-ext-coverage-etiquette.md](amenable-ext-coverage-etiquette.md).

## Problem

An amenable-ext checklist has one row per inventory item. For a generic type
such as `chrono::DateTime<Tz>` that hides which concrete instantiations are
covered: `ExtStandard<T>` takes a concrete `T`, and each instantiation has its
own witnesses. Today one `DateTime<Utc>` witness can make the whole generic
read Complete, because every registry lookup runs
`type_path_without_generics` (`framework_std/match_impl.rs`,
`framework_std/registry.rs`) and collapses all instantiations to one key.

Wanted: one row per concrete instantiation, each with its own status, plus an
aggregate generic row that notes what the type itself requires of its
parameters and which instantiations meet it.

**Principle: rustdoc, not markup.** Everything cordial knows about a
third-party type comes from rustdoc and the type's own definition. Amenable's
registry is trusted for exactly two things: evidence names
(`ExtStandard<T>`, which carry the full type with its arguments) and the
witnesses recorded against them. Generic-claim markup (`ExtGeneric<..>`,
`bounds`, `premises`) was tried and removed from amenable as a mistake: it
cannot be written for every third-party generic, and it cannot express
per-parameter bounds (`HashMap<K, V>`). An older dump that still carries it
loads and the markup is ignored.

## Findings that shape the design

- Registry evidence is already instantiation-bearing
  (`ExtStandard<chrono::DateTime<chrono::Utc>>`). **Covered** instantiations
  are discoverable from the registry with no config; cordial just discards the
  arguments today.
- **Missing** instantiations are not found in the registry (no entry exists to
  derive a row from), but rustdoc can supply them. A generic type declares
  what its parameters must satisfy (`struct DateTime<Tz: TimeZone>`), and
  rustdoc lists which types implement a trait. The types that implement the
  declared bounds, in the crates cordial is allowed to read, are the
  instantiations to expect. On the real data, `TimeZone` is implemented by
  exactly `Utc`, `Local`, `FixedOffset` (chrono) and `Tz` (chrono_tz). An
  earlier draft of this document said they were not discoverable; that was
  wrong.
- Path matching is currently heuristic (`type_has_trait_impl` falls back to
  bare-tail comparison). That is unsafe once a false match turns a Missing row
  Complete.

## Decisions

### 1. Expected instantiations are derived from rustdoc; `cordial.toml` overrides

So that generic support works on any library without anyone writing a list,
the default is derivation (`derive.rs`):

1. For each parameter, the candidates are the concrete types with a direct
   impl of **every** trait the type declares on that parameter, intersected,
   within the allowlisted crates (`resolve_crates`).
2. The instantiations are the product of one candidate per parameter, capped
   (default 16) so `K: Hash + Eq` across a whole std cannot generate thousands
   of rows. Over the cap, or with a parameter that declares no bounds, or with
   no implementor found, nothing is generated and the parent's note says why
   and asks for a list. It never guesses.
3. Rows are labelled by shortest public path (`chrono::DateTime<chrono::Utc>`),
   not the defining path through a private module.
4. The parent's note records the provenance: "Instantiations derived from the
   implementors of `Tz: TimeZone`".

Configuration is an override, per generic type, as full argument tuples:

```toml
[[amenable_ext.target]]
name = "chrono"
# Crates cordial may follow when resolving paths and looking for implementors;
# anything else is opaque.
resolve_crates = ["chrono", "chrono_tz"]

[amenable_ext.target.instantiations]
# An entry replaces the derivation for that type.
"chrono::Date" = [["chrono::Utc"], ["chrono::FixedOffset"]]
# An empty entry turns it off: no instantiation rows for this type.
"chrono::SomeOther" = []
```

Registered instantiations that were not derived or listed are still shown,
flagged "unexpected", so neither source can hide coverage. Rejected:
annotations in the amenable registry (the markup was removed as a mistake) and
hardcoding per-crate knowledge in cordial. `chrono::Date<Tz>` is in scope
because amenable keeps it as public API despite the deprecation; deriving
gives it the same four rows with no entry at all.

### 2. Canonical type identity (prerequisite phase)

Replace string matching for ext targets with structured identity.

- `TypeKey` = `(canonical generic id, [canonical arg keys])`; recursive.
- **Canonical id** is the defining path from rustdoc `krate.paths`
  (e.g. `chrono::offset::Utc`).
- **Alias (homonym) set** per canonical id: public re-exports
  (`chrono::Utc`), cross-crate spellings, and the bare names evidence strings
  use. Built from rustdoc `Use` items (`rustdoc/public_extract/reexport.rs`
  already collects same-crate re-export aliases) across the target crate and
  its dependency set (`chrono_tz::Tz` is defined outside chrono).
- Evidence names are **parsed** into a structured type, then each path
  resolved through the alias map; config entries go through the same
  resolver. Both sides of every comparison share one code path.
- **Cross-crate resolution is demand-driven and bounded**, so it can't drag in
  the dependency graph:
  - Resolve only paths that appear in registry evidence or the config list;
    never build an alias map for a whole crate.
  - Only crates in the target's `resolve_crates` allowlist are followed. A
    path outside it is an **opaque** canonical id (as-spelled path plus crate
    name): it matches by identity but is never expanded.
  - Re-exports are the same item under another name: follow them one name at a
    time with a visited set (cycles, diamonds).
  - Type aliases (`type Foo = Bar<Baz>`) expand to a type, possibly with
    arguments. Expand only inside allowlisted crates, under two caps; on
    either, report `Unresolved` naming the cap and path. Planned defaults
    for `cordial.toml` (added in phase A, when the config struct reads them;
    a target may override either):

    ```toml
    [amenable_ext]
    alias_depth = 8      # max chained type-alias expansions per name
    max_type_nodes = 64  # max nodes in one expanded TypeKey
    ```

    Measured on 2026-10-08 against the cached rustdoc JSON for chrono,
    chrono_tz and jiff: 4 type aliases in all (all in chrono; chrono_tz and
    jiff have none), longest alias chain 1, largest expanded alias 3 type
    nodes, longest re-export chain 1 in all three. The defaults leave wide
    headroom for other crates and are kept; the measurements are the floor,
    not the target.
  - Parse each allowlisted crate's rustdoc JSON once per run and index it
    lazily.
- Ambiguity (a name mapping to several canonical ids) and failure to resolve
  surface as an explicit `Unresolved` status, never a silent match or miss.
- `TypeKey` / resolver sit behind a trait seam (CLAUDE.md: traits at seams);
  alias set built by an enricher on the graph IR.
- Blast radius: `match_impl.rs` is shared with the std path. Introduce the
  resolver beside it, use it for ext targets only, migrate std separately.

### 3. Parent/child rows (amenable's model)

- Generic type = **aggregate parent** row; each instantiation = **child** row
  (a synthetic node with an `Instantiates` edge to the parent in the IR).
- The parent is Complete exactly when all its instantiation rows are Complete;
  otherwise Partial (n of m). Amenable's count: 67 checklist rows = 59 types +
  8 instantiation rows (`DateTime` and `Date`, four zone types each).
- The parent's note says what the type **declares** about its parameters
  (`struct DateTime<Tz: TimeZone>`, read from rustdoc's generics and `where`
  clauses, per parameter) and which instantiations meet it. A type that
  declares none says so. It is a note, never a child row.
- Kani has per-instantiation witness types only, so witnesses attribute to
  children directly.
- Bound satisfaction (`Utc: TimeZone`) is a direct-impl lookup in rustdoc,
  against the argument at the parameter's own position; unresolved or
  unchecked means "unknown", never covered.
- Depth capped at one level (`DateTime<Tz>`); nested generics deferred.

### 4. Reporting

Markdown: parent row, children indented beneath. CSV: explicit `parent_id`
and `kind` (`generic` | `instantiation` | `plain`) columns so consumers can
regroup. `Partial` is a new status value and touches every consumer of
`AmenableStdStatus`; audit them in phase C.

## Cordial requests from amenable

| # | Request | Needed by | Status |
| --- | --- | --- | --- |
| 2 | ~~Read `bounds` and `premises` from the registry dump~~ | withdrawn | Amenable removed the markup; cordial reads bounds from rustdoc instead |
| 3 | Shared dump's feature set includes `chrono` and `chrono-tz` | amenable Phase 1 | **Done** (`AMENABLE_DUMP_REGISTRY_FEATURES`) |
| 5 | Normalize whitespace in evidence names before matching | amenable Phase 1 | **Done** (`normalize_type_text`) |
| 1 | Eight instantiation rows, aggregates derived from them | end of amenable Phase 3 | Not started (phases A-C below) |
| 4 | Per-size rkyv results so a Complete names its size | amenable Phase 9 | Not started; separate plan when needed |

## Phases

| Phase | Work | Status |
| --- | --- | --- |
| 0 | Registry read side: whitespace normalization, chrono features in the dump. (The `bounds`/`premises`/`ExtGeneric` handling first built here was removed with amenable's markup.) | **Done** |
| A | `TypeKey`, rustdoc name lookup (re-exports, globs, aliases), structured evidence parsing, `Unresolved` status; ext targets only | **Done.** `framework_std::type_identity` (`TypeKey`, `CrateIndex`, `RustdocTypeResolver` behind the `TypeResolver` trait, `resolve_ext_evidence`); 18 tests including one against the real chrono / chrono_tz rustdoc JSON. Not yet wired into `evidence_for_ext_type`; that is phase B/C |
| B | Parent/child rows, aggregate roll-up, declared-bounds note, instantiations derived from rustdoc | **Done (library layer).** `framework_std::instantiation` (`expand_report`, `InstantiationEvidence`, `AmenableRegistryEvidence`, `ExpectedInstantiations`); `TypeResolver::{implements, declared_bounds, parameters, implementors, display}`; 21 tests including two on the real chrono / chrono_tz JSON (one with no configuration and an empty registry). Not yet wired into the assessor or the reporters; that is phase C |
| C | `instantiations` config key, Missing children, `Partial`, reporter/CSV changes | Not started |
| D | Chrono checklist verification (8 instantiation rows, 67 total) | Not started |

### Phase A as built

Resolution is a name lookup, not an alias map: `CrateIndex::lookup` walks the
segments of one path through modules, `pub use` and glob re-exports with a
visited set, so the work is proportional to the names asked about and glob
cycles terminate. Alias expansion substitutes type arguments and is bounded by
`alias_depth` and `max_type_nodes`. A path under a crate outside the allowlist
is opaque (kept as spelled, args still resolved). A bare name is searched in
every allowlisted crate and is `Ambiguous` when several distinct types answer.
Measured on the real chrono / chrono_tz JSON: the four `DateTime<..>`
instantiations resolve to four distinct keys with one shared head, and
`chrono::Duration` resolves through chrono's alias to the same key as
`chrono::TimeDelta`.

`resolve_ext_evidence` turns each `ExtStandard<..>` link into an
`EvidenceKey`. A link whose type cannot be identified is returned with its
reason, never dropped (a test caught the first draft dropping malformed
names).

### Phase B as built

`expand_report` replaces each generic row with an aggregate parent followed by
one child row per instantiation, recounting the report. Children are the
expected instantiations (configured order, phase C supplies the list) followed
by any registered instantiation that was not expected, noted as such. A child
is classified by the same function as an inventory row (`entry_from_facts`,
now shared), against an `InstantiationEvidence` source (see below), so
a witness is attributed to exactly the instantiation it proves. An expected
type that cannot be identified is a `Missing` child carrying the reason.

**The evidence source is a trait.** The expansion depends only on
`InstantiationEvidence`: which instantiations a source knows about, the
evidence record for one, the verifiers with a witness for it, and whether a
proof test names it, all asked in terms of `TypeKey`. Amenable's names
(`ExtStandard<T>`, proof records, proof-chain tests) live in exactly one place,
`AmenableRegistryEvidence`, the concrete implementation, which resolves the
registry once so a lookup compares identities. Nothing else in type identity,
rustdoc reading or the expansion mentions amenable, and a test drives the whole
expansion from a fixed-coverage source with no registry in it. One residue is
stated rather than hidden: the rows it produces are still the existing report
type, whose witness columns are `kani`, `creusot` and `verus`; a
library-neutral row model is separate, larger work.

Parent roll-up: Complete when every non-excepted child is Complete, Missing
when none has any coverage, Partial otherwise, Skipped when all are excepted;
a witness column is set only if every counted child has it. The parent's note
reads "3 of 4 instantiations Complete (1 excepted)" and then, from rustdoc
alone, what the type declares: "declared bounds `Tz: TimeZone`: satisfied by
Utc, FixedOffset, Local, Tz; no direct impl found for Bare (Tz: TimeZone)".

`CrateIndex::declared_bounds` reads the bounds a struct, enum or union puts on
its own parameters, from inline bounds and `where` clauses, and positions each
by the parameter's index so it is checked against that instantiation
argument. `Map<K, V>` with `K: Hash + Eq` and `V: Clone` is checked per
parameter and a failure names which: "(K: Eq)". `TypeResolver::implements`
checks one bound against a type's direct trait impls. Limits, stated in the
answer rather than hidden:

- Bounds a type does not itself declare (`HashMap` keeps `K: Eq + Hash` on its
  impl blocks) are not known. The note says the type declares none; it never
  guesses.
- Blanket impls are not attached to a type in rustdoc JSON, so absence is "no
  direct impl found", not "does not implement".
- An item is found from its canonical head by rustdoc's own defining-path
  table, because the defining path often runs through a private module that
  rustdoc strips from the tree. The real chrono data showed this, not the
  synthetic fixture, and it has its own regression test.

## Open questions

- Evidence names are `stringify!` of the type as written in amenable source,
  so paths may be bare or re-exported; confirmed format in amenable commit
  `ff915c64`. Whether amenable will treat the spelling as a stable contract is
  unconfirmed; phase A's resolver is the defence.
- Cross-crate alias resolution: which rustdoc JSONs the `resolve_crates`
  allowlist needs (shadow-dep rustdoc already exists per target).
- Multi-parameter generics: tuple syntax above assumed; no current target
  exercises it.

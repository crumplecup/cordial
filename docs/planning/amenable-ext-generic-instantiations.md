# Generic instantiations as coverage rows (amenable-ext)

Status: **Active**

Origin: downstream amenable report "Report generic type instantiations as
separate coverage rows". Builds on
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

Wanted: one row per concrete instantiation, each with its own status, plus a
generic row that carries generic claims (`impl<Tz: TimeZone>`) and notes which
instantiations they cover.

## Findings that shape the design

- Registry evidence is already instantiation-bearing
  (`ExtStandard<chrono::DateTime<chrono::Utc>>`). **Covered** instantiations
  are discoverable from the registry with no config; cordial just discards the
  arguments today.
- **Missing** instantiations are not discoverable: no registry entry exists to
  derive a row from, and rustdoc shows `DateTime<Tz>`, not its uses. They need
  an expected list.
- Path matching is currently heuristic (`type_has_trait_impl` falls back to
  bare-tail comparison). That is unsafe once a false match turns a Missing row
  Complete.

## Decisions

### 1. Expected instantiations live in `cordial.toml`

Per-target, per-generic, full argument tuples (not per-parameter option lists,
to avoid a combinatorial product for multi-parameter types):

```toml
[[amenable_ext.target]]
name = "chrono"
# ...existing keys...
# Crates cordial may follow when resolving paths; anything else is opaque.
resolve_crates = ["chrono", "chrono_tz"]

[amenable_ext.target.instantiations]
"chrono::DateTime" = [["chrono::Utc"], ["chrono::FixedOffset"], ["chrono::Local"], ["chrono_tz::Tz"]]
```

Rejected: hardcoding in cordial (per-crate knowledge; cordial stays
target-agnostic) and annotations in the amenable registry (couples the
checklist to another repo). Deprecated types such as `chrono::Date<Tz>` are
not listed; they are a case for cordial's exceptions mechanism. The list is optional: with none, a generic keeps
its single row plus any instantiations discovered from registry evidence.
Registry-discovered instantiations not in the list are still shown, flagged
"unexpected", so the list can't hide coverage.

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

    The values are unmeasured guesses until the alias-chain scan in phase A.
  - Parse each allowlisted crate's rustdoc JSON once per run and index it
    lazily.
- Ambiguity (a name mapping to several canonical ids) and failure to resolve
  surface as an explicit `Unresolved` status, never a silent match or miss.
- `TypeKey` / resolver sit behind a trait seam (CLAUDE.md: traits at seams);
  alias set built by an enricher on the graph IR.
- Blast radius: `match_impl.rs` is shared with the std path. Introduce the
  resolver beside it, use it for ext targets only, migrate std separately.

### 3. Parent/child rows

- Generic type = **parent** row; each instantiation = **child** row (a
  synthetic node with an `Instantiates` edge to the parent in the IR).
- Parent holds generic claims; children hold concrete witnesses.
- Many-to-one: a generic claim covers many children. Each child carries
  `direct` status (own witnesses) and `inherited` status (generic claims whose
  bounds it satisfies). Displayed status derives from both: Complete (direct),
  Complete (via generic), Missing, Unknown.
- Bound satisfaction (`Utc: TimeZone`) comes from rustdoc impl generics. When
  a bound can't be resolved the child is **Unknown**, not covered.
- Parent roll-up: Complete if a generic claim covers it, or all expected
  children Complete; otherwise **Partial** (n of m). The parent's note lists
  which instantiations each generic claim covers (computed from the edges).
- Depth capped at one level (`DateTime<Tz>`); nested generics deferred.

### 4. Reporting

Markdown: parent row, children indented beneath. CSV: explicit `parent_id`
and `kind` (`generic` | `instantiation` | `plain`) columns so consumers can
regroup. `Partial` is a new status value and touches every consumer of
`AmenableStdStatus`; audit them in phase C.

## Phases

| Phase | Work | Status |
| --- | --- | --- |
| A | `TypeKey`, alias set from rustdoc, structured evidence parsing, `Unresolved` status; ext targets only | Not started |
| B | Parent/child nodes, direct vs inherited status, bound checking | Not started |
| C | `instantiations` config key, Missing children, `Partial`, reporter/CSV changes | Not started |
| D | Chrono checklist verification (4 `DateTime<Tz>` instantiation rows) | Not started |

## Open questions

- Confirm with the amenable side that evidence names will keep carrying
  generic arguments in the registry dump.
- Cross-crate alias resolution: which rustdoc JSONs the `resolve_crates`
  allowlist needs (shadow-dep rustdoc already exists per target) and
  whether the default caps hold up against a scan of chrono, chrono_tz and
  jiff.
- Multi-parameter generics: tuple syntax above assumed; no current target
  exercises it.

# Modularity report/checklist open-item count mismatch

Status: **Done**

## Problem

`cordial open` (and any other command that builds or rebuilds the worklist)
fails every run with:

```
Error:   x invariant violated: Modularity: report has 3 open items but checklists
  | have 2
```

This is `Worklist::build_with_tiers`'s own consistency check
(`src/worklist/state.rs:96-103`): for each quality area it compares
`area.open_items()` (the report's own count) against
`source.items(area)?.len()` (items actually parsed back out of the written
checklist markdown), and refuses to build a list when they disagree. Reported
by the amenable agent while dogfooding `cordial open`/`cordial run` on the
`amenable` project; this is unrelated to the Modularity `report has N open
items but checklists have M` invariant failure reported and patched earlier
in the same session -- that one is believed fixed, this is a different,
previously-latent edge case the fix did not touch.

## Root cause

Two independent pieces of code both claim to count Modularity's open
checklist items, and they count different things:

- `src/etiquettes/modularity/quality_area.rs`'s `metrics()` (used by
  `quality_area_compute`, which supplies `area.open_items()` for the report)
  sums every `checklist == "true"` finding independently, by kind, across the
  whole area: `large_files + large_functions + types_per_file + top_heavy +
  lopsided + collapse` (lines ~37-62).

- `src/etiquettes/modularity/reporter/checklist.rs`'s `file_hotspots()` /
  `render_crate_checklist()` *nests* a file's `MODULARITY-TYPES-PER-FILE` (and
  `MODULARITY-FUNCTION` / `MODULARITY-TOP-HEAVY` / `MODULARITY-LOPSIDED` /
  `MODULARITY-COLLAPSE`) finding as a sub-bullet under that same file's
  `MODULARITY-FILE` entry, when that file already has one (lines 393-436,
  rendered at lines 206-216) -- deliberately, so one oversized file doesn't
  get two separate top-level checklist bullets. `src/worklist/source.rs`'s
  `parse_checklist` only counts unindented `- [ ] ` lines, so a nested
  sub-bullet is never a separate `WorklistItem`.

Whenever a file is *both* a `MODULARITY-FILE` hotspot and has at least one
other `checklist=true` finding that would nest under it (most commonly
`MODULARITY-TYPES-PER-FILE`), `metrics()` counts it as two-or-more open
items while the rendered checklist gives it exactly one bullet. The report
and the checklist's own notion of "open item" diverge by exactly the number
of nested sub-findings across the whole area.

## Reproduction (confirmed on `amenable`, 2026-10-22)

`findings/modularity.csv` has exactly two `checklist=true` rows of kind
`MODULARITY-FILE` (for `amenable_kani`'s `gallery::
chrono_offset_date_time_round_trip` and `gallery::boxed_error_variant_count`)
plus one `checklist=true` row of kind `MODULARITY-TYPES-PER-FILE` for
`gallery::boxed_error_variant_count` (33 types, over
`max_types_per_file`). `metrics()` sums these to 3 (`large_files: 2,
types_per_file: 1`). `findings/modularity.checklist.md` renders the
types-per-file finding as a "peel types: **33 types**" sub-bullet nested
under that file's own `- [ ]` entry (see `file_hotspots`'s `types` field),
so `parse_checklist` sees only 2 top-level items. The project's own working
tree was clean at the time; this is not an artifact of in-progress edits on
the amenable side, only of one file happening to cross both the file-size
and the types-per-file checklist thresholds in the same run.

## Fix direction

Not prescribing an implementation, since the right fix depends on which side
should move: either `metrics()` should stop counting a nested
`MODULARITY-TYPES-PER-FILE`/`FUNCTION`/`TOP-HEAVY`/`LOPSIDED`/`COLLAPSE`
finding separately once it would be folded into an owning `MODULARITY-FILE`
hotspot bullet (mirroring `file_hotspots`'s own nesting decision), or
`checklist.rs` should give every checklist=true finding its own top-level
bullet and stop nesting (simpler, but changes the checklist's current,
intentionally compact rendering for oversized files). Whichever direction is
chosen, the two should share one source of truth for "how many open items
does this area have," rather than recomputing the count twice via two
independently-maintained tallies that can drift whenever a new nesting case
(like this one) shows up.

## Status

- [x] Design: the checklist renderer owns the top-level item count
- [x] Implementation: the quality area uses the checklist renderer's count
- [x] Regression test: a fixture file that is both a `MODULARITY-FILE` hotspot
      and a `MODULARITY-TYPES-PER-FILE` hit, asserting `Worklist::build`
      succeeds and the item count matches on both sides

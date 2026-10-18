# Gate tiers

Status: **Active**

Group etiquettes into named tiers by cost, so a run can select the cheap ones
and leave the expensive ones for when they are wanted. Cordial ships built-in
tiers, partitioned from measured cold and warm timings, and users can define
their own.

## Problem

The only way to make a run cheaper today is to switch etiquettes off. That is a
policy decision (`[feature_warnings] enabled = false`), and it is a bad fit for
cost:

- `feature_warnings` runs `cargo hack check --feature-powerset`. It is the most
  expensive etiquette, so it ships off by default, and nothing but a config edit
  turns it on for the one run where it matters.
- Other etiquettes spawn processes too: `doc_warnings` runs `cargo doc`,
  `dependency_freshness` runs `cargo update`, `creusot_diagnostics` and
  `verus_warnings` invoke their tools. Each one is on, so every run pays for it,
  including the runs where an agent only wants to know whether a fix to an
  `unwrap` worked.
- `--etiquette <id>` selects by name. It is the right tool for one check and a
  poor one for "everything that finishes in a few seconds".

The [agent worklist](agent-worklist.md) makes the cost visible. An agent that
re-runs the gate after each fix pays for the slowest etiquette on every
iteration. If most of the gate is cheap, a run that includes only the cheap part
is a usable per-area check.

## Concepts

**Tier.** A name attached to a set of etiquettes. A run selects tiers, and an
etiquette runs when its tier is selected. Tier names are open strings, so a
user-defined tier is a new name and nothing more.

**Built-in tiers.** Three, ordered by cost:

| Tier | Meaning |
| --- | --- |
| `quick` | Cheap cold and warm; usually parsing, the graph, and assessment |
| `build` | Expensive cold setup or build, cheap once caches are warm |
| `deep` | Expensive even warm, or depends on the network or a toolchain that is slow to run |

**Selecting tiers.** One run selects one or more tiers. With no flag, the run
selects the project's default tiers (see Configuration). Three rules settle the
interactions with the existing selectors:

- `enabled = false` always wins. A tier never turns on an etiquette the project
  switched off.
- An explicit `--etiquette <id>` always wins over tiers. Naming a check runs it.
- A run with no explicit etiquette uses the intersection of the selected tiers
  and the enabled etiquettes.

**User-defined tiers.** A project can assign etiquettes to any tier name and
select that name on the command line. Typical uses: a `pre-commit` tier of the
five checks a team wants on every commit, or a `ci` tier for a nightly job.
Plugin authors can place their own etiquettes in the built-in tiers or in a tier
of their own.

## What each etiquette costs

The partition is data, not opinion. The first built-in assignment is made from
the measurements below, and the table is recorded in the repository so it can be
reviewed and changed.

Etiquettes that spawn a process, from reading the source:

| Etiquette | What it runs |
| --- | --- |
| `doc_warnings` | `cargo doc --no-deps` |
| `feature_warnings` | `cargo hack check --feature-powerset` |
| `dependency_freshness` | `cargo update` |
| `creusot_diagnostics` | the configured Creusot runner |
| `verus_warnings` | the `verus` binary |

`feature_warnings` is `deep`; that is settled. The rest, and the coverage
etiquettes that use rustdoc JSON, are placed by measurement. Everything else
starts in `quick` and moves only if measurement shows it does not belong.

## Timings

### Measuring

`cordial timings` runs each enabled etiquette on its own, twice, and writes the result to
the store:

1. **Cold.** An empty `CARGO_TARGET_DIR` in a temporary directory, so the run
   pays for every build it needs.
2. **Warm.** The same target directory again, with nothing changed.

It shows a spinner per etiquette per pass, through the same progress sink as
the other long commands (see "Progress on long commands" in the
[agent worklist](agent-worklist.md)), because a cold pass can take minutes.

It runs against the project given by `-p`, so users measure their own code as
well as cordial's. Output is `timings.md` for people and `timings.json` for the
partitioner, with wall time per etiquette for each pass.
`--etiquette <id>` narrows a measurement to one enabled etiquette. Timing
passes read the normal project store and may refresh build caches, but do not
replace quality reports, checklists, or the last-run ledger.

### Standalone cost

An etiquette is measured alone. Loaders and enrichers are shared between
etiquettes, so inside a full run the first user of a shared loader pays for it
and the rest do not. Standalone cost is what an agent pays when it selects a
tier, so the tier table uses it. The sum of standalone costs overstates the
cost of a tier run, and that is the safe direction to be wrong in.

### Partitioning

Two thresholds split the data:

| Condition | Tier |
| --- | --- |
| warm time is above the warm threshold | `deep` |
| cold time is above the cold threshold, warm time is not | `build` |
| neither | `quick` |

The warm threshold separates "cheap to repeat" from "costly every time". The cold
threshold separates "no build involved" from "pays for a build the first time".
Both are constants chosen after the first measurement pass, at the visible gaps in
the data instead of at round numbers, and then recorded with the measurements
they came from.

The 2026-10-17 runs measured Cordial and the larger Amenable workspace. The
complete per-etiquette results are in each project's default-store `timings.md`
and `timings.json`. These rows determined the first thresholds (seconds):

| Etiquette | Cordial cold/warm | Amenable cold/warm | Initial tier |
| --- | ---: | ---: | --- |
| `tracing` | 5.97 / 5.90 | 6.19 / 6.01 | `quick` |
| `antipatterns` | 2.95 / 2.91 | 252.74 / 4.79 | `build` |
| `amenable-ext-*` | unavailable | 39.78–57.52 / 2.01–3.73 | `build` |
| `doc_warnings` | 41.08 / 2.07 | 146.15 / 3.05 | `build` |
| `dependency_freshness` | 2.58 / 2.31 | 9.54 / 9.53 | `deep` |
| `impl-coverage` | 59.04 / 2.15 | 157.75 / 10.73 | `deep` |
| `verus_warnings` | 1.97 / 1.97 | 33.92 / 34.10 | `deep` |
| `amenable-std` | unavailable | 39.55 / 38.04 | `deep` |

The warm threshold is **8 seconds**, between the measured 6.19-second source
checks and 9.53-second recurring work. The cold threshold is **20 seconds**,
between the 9.54-second checks and 33.92-second builds or tool runs. A warm
pass above 8 seconds wins even when its cold pass is also above 20 seconds.
`feature_warnings` remains `deep` by design; it was disabled during these runs
because its feature powerset is expensive. `creusot_diagnostics` is provisionally
`deep` because neither checkout exercised a representative proof run.
`homecoming-std` is provisionally `build` because neither checkout supplied
the assessor findings needed to measure it; its rustdoc prerequisite has the
same cold-build shape as the measured coverage checks.

Measured results go into a table of built-ins, a `const` in the same style as the
ordering table. Cordial merges it at runtime with the tier assignments from
`cordial.toml`, as it does for ordering rows. Measuring does not change tiers by
itself; a person reads `timings.md` and updates the table or the config.

## Configuration

```toml
[tiers]
# Tiers a run selects when no --tier flag is given.
default = ["quick", "build"]

# Reassign an etiquette, or place it in a tier of your own.
[tiers.assign]
dependency_freshness = "ci"
doc_warnings = "deep"
glob_imports = "pre-commit"
panics = "pre-commit"
```

- `default` names the tiers a bare `cordial run` or `cordial quality` selects.
  Its default value is `quick` and `build`, which keeps `deep` opt-in.
- `assign` maps an etiquette id to a tier. It overrides the built-in table and
  the etiquette's own declaration. An id not in the table keeps its tier.
- Unknown tier names in `default` or on the command line are an error that lists
  the known tiers.

`feature_warnings` stops being off by default. It is on, in `deep`, and runs when
`deep` is selected. `enabled = false` still switches it off entirely.

## Command line

| Command | What it does |
| --- | --- |
| `--tier <name>[,<name>]` on `run`, `quality`, `coverage`, and `submit` | Select tiers for this run. `--tier all` selects every tier |
| `cordial tiers` | List tiers, their etiquettes, and where each assignment came from (built-in, plugin, config) |
| `cordial timings` | Measure cold and warm cost per etiquette and write `timings.md` and `timings.json` |

`cordial explain <etiquette>` gains the etiquette's tier, and plain
`cordial explain` shows it in the list.

## What a partial run means for reports

A tier that did not run has found nothing, and it has not passed. The reports
have to keep those two apart.

- The quality report shows an area whose etiquettes were not selected as
  "not run (tier `deep`)" with the open count from its last run, if there was
  one. It does not show zero.
- Reporters write checklists only for etiquettes that ran. A checklist from an
  earlier run stays on disk and is marked stale in the report.
- A small ledger, `{store}/ledger.json`, records for each etiquette its tier, the
  time of its last run, its open count, and its wall time. The report reads it for
  the "last run" line, and `cordial timings` reads it to compare against.
  Until tier selection is implemented, the ledger records `unassigned`. Its
  wall time sums the etiquette's measured hooks; a shared hook is attributed to
  each etiquette that uses it. The isolated `timings` command will measure the
  complete cost of one etiquette, including shared setup.
- `--deny-open` judges only what ran, and prints a line naming any tier that was
  skipped. The promotion gate uses `--tier all`, so a skipped tier cannot reach
  `main` unnoticed.

## Library design

New module `src/tier.rs`, a barrel of `mod` and `pub use`, with named siblings
under `src/tier/`. Names avoid `Standard`.

| File | Holds |
| --- | --- |
| `name.rs` | `Tier`: a name type with `quick`, `build`, and `deep` constants |
| `table.rs` | the built-in `const` assignment table and its runtime merge with config |
| `select.rs` | `TierSelection`: default tiers, the `--tier` value, the intersection rules |
| `timing.rs` | timing records and the cold/warm measurement driver |
| `ledger.rs` | `Ledger`: per-etiquette last-run records |
| `partition.rs` | the thresholds and the function that turns timings into tiers |

Changes to existing code:

- `Etiquette` gains `fn tier(&self) -> Tier` with a default of `quick`. Existing
  plugins compile unchanged. Built-ins override it only through the table, and a
  plugin author overrides it when the etiquette spawns a process.
- `RunFilter` gains `tiers()`, returning `None` by default, and
  `NamedRunFilter` gains `with_tiers`.
- `run_session` in `src/session/run.rs` applies the tier selection next to the
  existing `config.etiquette_enabled(...)` filter. That is the single place where
  an etiquette is dropped from a run for configuration reasons, so tiers belong
  beside it.
- `CordialConfig` gains a `tiers` table, loaded and layered like the others.
- The session records wall time per hook, keyed by hook id, so `cordial timings`
  and the ledger share one source.

## Interaction with the worklist

The tier selection becomes part of the worklist snapshot, replacing the
"plugin set" the [agent worklist](agent-worklist.md) planned to record.

- `submit` repeats the tiers the previous run used, and `submit --tier <name>`
  overrides them. An agent can verify an area cheaply with `--tier quick`, and
  finish with `submit --tier all`.
- Items from a tier that this `submit` did not run carry over from the previous
  worklist unchanged and are marked unverified. They cannot be scored, and they
  are never reported as cleared.
- `stats` states which tiers the current worklist covers.

## Phases

1. **Record timings.** Per-hook wall time in the session, and the ledger. No
   behavior change.
2. **`cordial timings`.** Cold and warm measurement, `timings.md` and
   `timings.json`. Run it on cordial and on at least one larger workspace.
3. **Built-in table.** Choose the thresholds from the data, record them with the
   measurements, and write the `const` table.
4. **Selection.** `Tier`, `Etiquette::tier`, `RunFilter::tiers`, the `[tiers]`
   config table, `--tier`, and the filter in `run_session`. Flip
   `feature_warnings` to on, in `deep`.
5. **Reports.** "Not run" areas, stale checklists, and the `--deny-open` line for
   skipped tiers. `just cordial-gate` and the promotion checklist use `--tier all`.
6. **`cordial tiers` and explain output.**
7. **Worklist integration.** After the worklist phases that read the snapshot.

## Verification

- Tests under `tests/` for tier resolution: default tiers, `--tier` unions,
  `enabled = false` beating a tier, `--etiquette` beating a tier, unknown tier
  names, and config assignment overriding the built-in table.
- A report test for an unselected area: it shows "not run" and its previous
  count, and never zero.
- A partitioner test over fixed timing fixtures, so threshold changes show up as a
  diff in expected tiers.
- `just check-all`, `just check-features`, and `just cordial-gate` before
  promoting `dev`, with the gate run at `--tier all`.

## Decisions to settle

- **Default tiers.** `quick` and `build` keeps today's behavior apart from
  `feature_warnings`, and keeps `deep` opt-in. The alternative is `quick` alone,
  which makes the default run much cheaper and moves more checks behind a flag.
- **Tier names.** `quick`, `build`, and `deep` describe cost. If the `build` name
  suggests the etiquette compiles the user's code, `cargo` is a closer fit.
- **Where timings run.** `cordial timings` uses a temporary target directory so it
  never disturbs the project's own. That costs a full cold build per measured
  project, which is the point, but it is also the reason to run it rarely.

## Status

- [x] Phase 1: per-hook timing and the ledger
- [x] Phase 2: `cordial timings` (measured Cordial and Amenable on 2026-10-17;
  unavailable etiquette measurements are recorded in their store reports)
- [x] Phase 3: thresholds and the built-in table
- [ ] Phase 4: selection, config, `--tier`
- [ ] Phase 5: report handling for tiers that did not run
- [ ] Phase 6: `cordial tiers` and explain output
- [ ] Phase 7: worklist integration

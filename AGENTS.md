# Agent Instructions

`cordial` is the authority for this repository's coding standards.

Start with the smallest source that answers the task. The compiled instructions
and explanations are the policy the tool actually runs.

## Authority

1. `cordial instructions` is authoritative for the agent worklist workflow.
   `cordial explain <id-or-rule-id>` is authoritative for etiquette behavior,
   finding meaning, and opt-out guidance.
2. `cordial open`, `cordial stats`, and `cordial submit` show the current
   worklist and results. `cordial quality` and `cordial run` reports show the
   findings behind it.
3. Project planning notes belong under `docs/`. Use local planning docs
   for architecture, naming, and active work.
4. Other repository docs explain usage and background.

## Quick Use

```sh
cordial instructions
cordial open
cordial done <id>       # after fixing the item and running focused checks
cordial submit
```

1. Read `cordial instructions` once, then use `cordial open` for the next item.
   The first build can take a few minutes; allow a long timeout.
2. Run the item's suggested `cordial explain` command before changing code,
   suppressing a finding, or reinterpreting a rule. Fix the item and run focused
   project checks before `cordial done <id>`. Use `cordial skip <id>` when you
   cannot act on it now.
3. When the list is complete, run `cordial submit` and wait for it. Continue
   until submit reports zero open items. `cordial stats --verbose` shows list
   state. Use item ids when issuing several worklist commands concurrently.
4. Before each commit, run `just cordial-gate` (or
   `cordial quality -p . --deny-open --tier all`) and clear every open item, including
   dependency freshness. This is also the promotion check for `dev` to `main`.

Work from `open` and `stats`; raw checklists under `findings/` do not include
progress on the current list. Use them for detail only when the item requires it.

Use `cordial explain` with no argument to list the compiled etiquettes.

Useful routing ids:

| Task | Start with |
| --- | --- |
| Error handling, panics, source chains | `cordial explain panics`, `cordial explain error_sites`, `cordial explain error_chain`, `cordial explain internal_error_chain`, `cordial explain foreign_error_types`, `cordial explain foreign_error_attenuation` |
| Observability and diagnostics | `cordial explain tracing`, `cordial explain allows`, `cordial explain doc_warnings`, `cordial explain crate_attrs` |
| API shape and public paths | `cordial explain visibility`, `cordial explain derives`, `cordial explain pageantry`, `cordial explain glob_imports` |
| File/module organization | `cordial explain modularity`, `cordial explain inline_tests`, `cordial explain cli_layout` |
| Conditional code | `cordial explain cfg_scatter`, `cordial explain cfg_hygiene` |
| Proof hygiene | `cordial explain verus_warnings`, `cordial explain creusot_diagnostics`, `cordial explain proof_patterns` |
| Coverage work | `cordial explain impl-coverage`, `cordial explain trenchcoat`, `cordial explain shadow`, `cordial explain homecoming-std`, `cordial explain amenable-std` |
| Dependency freshness | `cordial explain dependency_freshness` |

## Repo Map

| Need | Start with |
| --- | --- |
| Work through findings | `cordial instructions`, then `cordial open` |
| Commit or promotion gate | `just cordial-gate` (zero open items required) |
| Promote `dev` to `main` | `just check-all`, `just check-features`, `just cordial-gate`, then a PR; CI must be green |
| Understand a finding | `cordial explain <id-or-rule-id>` |
| Inspect list progress | `cordial stats --verbose` |
| Review config knobs | `cordial.toml`, then any config docs under `docs/` |
| Make structural changes | Planning docs under `docs/`, then relevant `cordial explain <id>` pages |
| Track active project plans | Planning indexes and status docs under `docs/` |
| Learn local project context | `README.md`, then focused docs under `docs/` |

## Working Rules

- Keep generated cordial artifacts under the configured store
  (`~/.cordial/{project}/` by default); do not commit them.
- Prefer narrow, etiquette-driven changes over broad cleanup.
- For pipeline analysis and `--apply`, collect source facts through the shared
  loader and enrich the shared IR. When a needed fact is missing, extend the IR
  and its collection/enrichment path. Probes, assessors, reporters, and apply
  must consume those facts from `IrView`; do not add a standalone source scan
  to reconstruct them. Apply may read source text to make located edits and
  parse proposed edits for validation.
- For Rust projects, put tests under `tests/` unless an existing local pattern
  requires otherwise.
- Before finishing code changes, run the focused Cargo checks needed for the
  touched area. Run the zero-open Cordial gate before every commit.

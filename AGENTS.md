# Agent Instructions

`cordial` is the authority for this repository's coding standards.

Start with the smallest source that answers the task. Prefer the compiled
explanations because they are the policy the tool actually runs.

## Authority

1. `cordial explain <id-or-rule-id>` is authoritative for etiquette behavior,
   finding meaning, and opt-out guidance.
2. `cordial quality` and `cordial run` reports are authoritative for this
   checkout's current findings.
3. Project planning notes belong under `docs/`. Use the local planning docs
   for architecture, naming, and active work.
4. Other repository docs explain usage and background.

## Quick Use

```sh
cordial explain
cordial quality -p .
cordial view findings/quality-report.md
```

1. Run `just cordial-gate` (or `cordial quality -p . --deny-open`) to produce
   the local quality report and fail if open action items remain.
2. Open `findings/quality-report.md` with `cordial view` and start from the
   open action items.
3. Follow the report links to the relevant checklist artifacts.
4. For each finding, run `cordial explain <id>` or
   `cordial explain <rule-id>` before changing code, suppressing an item, or
   reinterpreting the rule.
5. After edits, rerun the focused project checks and the cordial command that
   produced the finding.

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
| Run the standards | `just cordial-gate`, then `cordial view findings/quality-report.md` |
| Promote `dev` to `main` | `just check-all`, `just check-features`, `just cordial-gate`, then a PR; CI must be green |
| Understand a finding | `cordial explain <id-or-rule-id>` |
| Find the next checklist | Links inside `findings/quality-report.md` |
| Review config knobs | `cordial.toml`, then any config docs under `docs/` |
| Make structural changes | Planning docs under `docs/`, then relevant `cordial explain <id>` pages |
| Track active project plans | Planning indexes and status docs under `docs/` |
| Learn local project context | `README.md`, then focused docs under `docs/` |

## Working Rules

- Keep generated cordial artifacts under the configured store
  (`~/.cordial/{project}/` by default); do not commit them.
- Prefer narrow, etiquette-driven changes over broad cleanup.
- For Rust projects, put tests under `tests/` unless an existing local pattern
  requires otherwise.
- Before finishing code changes, run the focused Cargo checks needed for the
  touched area and rerun the relevant cordial command when the change addresses
  a cordial finding.

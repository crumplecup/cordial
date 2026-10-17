# Running cordial

Use this page for day-to-day command behavior, store layout, config layering,
and exception workflow. Use [Built-in etiquettes](built-in-etiquettes.md) when
you need to interpret a finding.

## Install

```sh
cargo install --path .
# Quality-only binary:
cargo install --path . --no-default-features --features quality,cli
```

The default install uses the `full` feature set. Built-in bundles are
feature-gated on the `cordial` crate, including `panics`, `tracing`, `quality`,
`elicitation`, `homecoming_std`, `amenable_std`, and `full`.

## Common commands

```text
cordial instructions                 # compiled agent worklist workflow
cordial open                         # show the current item; build a list if needed
cordial done <id>                    # claim a fix and show the next item
cordial skip <id>                    # set an item aside until the next list
cordial peek                         # show the current item again
cordial stats --verbose              # counts and item states
cordial submit                       # rerun the gate and score claims
cordial quality -p <project>          # source-quality etiquettes
cordial quality --deny-open           # fail after reports if open action items remain
cordial quality --apply               # tracing recipes + crate-root lint attributes
cordial quality --apply --dry-run     # log apply without writing
cordial explain                       # id + one-line why for every compiled etiquette
cordial explain doc_warnings          # why it exists, what it flags, how to opt out
cordial explain DOC-WARNING-001       # same page (rule id alias)
cordial build rustdoc                 # rustdoc JSON for coverage
cordial build sysroot                 # std-family rustdoc for framework coverage
cordial coverage                      # impl / trenchcoat / shadow / std-family coverage
cordial run                           # quality + coverage
cordial run --deny-open               # CI gate for unresolved open findings
cordial view findings/quality-report.md
```

`-p` / `CORDIAL_PROJECT` selects the project root. `--crate-name` restricts a
run to one crate. `--store-home` / `CORDIAL_HOME` overrides `~/.cordial`.

## Agent worklist

Run `cordial instructions` for the full loop. `open` gives the highest-priority
item and builds a list if needed. After fixing it and running focused project
checks, use `done <id>` to claim it; use `skip <id> --reason "why"` if you
cannot act on it now. Skip reasons and the number of lists an item has survived
appear on later lists. A `STOP:` line asks you to tell the user when an item
reaches the configured pass limit (`[worklist] max_passes`, default 3).
`submit` reruns the gate, reports which claims cleared or reopened, and writes
the next list. Repeat until submit reports zero open items. The first `open`
may take several minutes, so give it a long timeout.

`peek` shows the current item again. `stats` summarizes counts by area;
`stats --verbose` lists item states, with `--area <title>` to narrow the view.
`stats` also shows list age and how many Rust sources or Cargo manifests changed
since it was built. `open` shows that hint once a list is at least a day old.
The hint does not rebuild the list; `submit` checks it when you are ready.
Name ids when sending several worklist commands at once. Run `submit` alone
and wait for it; commands issued while it runs wait for its new list.

The worklist is the agent-facing view of `quality-report.md`. Raw checklists
do not show checkoffs or the current cursor. `cordial explain <rule-id>` remains
the authority for what each finding means and how to document an exception.
For CI, commits, and promotion, use `cordial quality --deny-open` or
`just cordial-gate` and clear every open item.

## Progress and logs

Interactive CLI runs write progress to stderr with spinners and counted bars.
When stderr is not a terminal, long commands write start, finish, and heartbeat
lines instead. Set `CORDIAL_NO_PROGRESS=1` to silence progress.

Diagnostics still use tracing. CLI commands default to warning-level logs; set
`RUST_LOG=info` or `RUST_LOG=debug` when you want operational detail.

## Store

Reports write to `{store_home}/{project}/findings/`:

- CSV inventories;
- markdown checklists;
- markdown summaries;
- `findings/quality-report.md` for the quality rollup.
- `worklist.json` for the current agent list, claims, and generation.

Artifacts are local and regeneratable. They live under `~/.cordial/{project}/`
by default and should not be committed to the target repository.

## Config

Thresholds load later-wins:

1. `CordialConfig::default`;
2. `{store_home}/cordial.toml`;
3. `{workspace}/cordial.toml`.

Missing config files fall back to defaults. Every etiquette table accepts
`enabled` with a default of `true`; `enabled = false` turns that lint off for
the project:

```toml
[doc_warnings]
enabled = false
```

Canonical knobs are documented in the committed `cordial.toml` and
`docs/planning/cordial-config.md`.

## Exceptions

Hand-audited exceptions live in the target repo for review and CI, then copy
into the local store before a run:

```sh
cordial exceptions load
cordial exceptions list
cordial exceptions show <etiquette>
cordial exceptions add <etiquette> --file src/lib.rs --reason "..."
cordial exceptions backup
```

Default registry path is `{project}/.cordial-exceptions`. Relative paths join
the project root, so this works from any current directory:

```sh
cordial -p /repos/elicitation exceptions load .elicit_doc-exceptions
```

The slug-scoped tree is:

```text
.cordial-exceptions/
  {slug}/
    exceptions/          # quality suppressions: {etiquette}/{crate}.json
    quality/patches/     # elicit_doc alias for the same files
    patches/             # coverage skip lists: {crate}.json, {crate}-shadow.json
```

`load` replaces those store subtrees, deleting stale files. Missing
`{root}/{slug}` is an error. `list` and `show` inspect what is in the store
after load.

Create one store row at a time with flags:

```sh
cordial exceptions add panics --file src/lib.rs --rule-id PANIC-SOURCE-PANIC --reason "intentional"
cordial exceptions add panics --file src/lib.rs --line 12 --context demo::boom --reason "fixture"
cordial exceptions add --patch-set chrono --path chrono::DateTime --reason "upstream skip"
```

`--crate-name` selects the quality JSON stem, defaulting to global
`--crate-name` or the project directory. `cordial exceptions backup` writes the
store back into the repo registry. The same append is available from the
library as `add_exception` and `add_coverage_skip`.

Remove or correct one quality row without hand-editing JSON. Select the row
with any combination of `--rule-id`, `--context`, `--file`, and `--line`; the
selector must match exactly one row, or the command errors and changes nothing
(an ambiguous match lists the candidate rows):

```sh
cordial exceptions remove visibility --rule-id VISIBILITY-A --context ext::jiff
cordial exceptions edit visibility --context ext::jiff \
    --new-context jiff --new-file src/jiff/mod.rs
```

`edit` keeps the row's position and any field you do not replace; an empty
`--new-rule-id` or `--new-context` clears that field. Removing the last row of a
crate's file deletes the file, so the next `backup` leaves no stale copy in the
registry. The library equivalents are `remove_exception` and
`update_exception`.

Exceptions match findings exactly (`context` included), so a refactor that
renames a module silently disables an exception. Every `run` and `quality`
logs a warning for each exception that matched no finding of its etiquette
(`exception matches no finding; remove or edit it`), naming the crate, file,
rule id, context, and reason. Fix it with `edit`, or `remove` it when the
condition it covered no longer exists.

## Quality

`cordial quality` runs every source-quality etiquette compiled into the binary.
Quality `--apply` rewrites source only for supported apply paths:

- tracing instrument recipes from the checklist;
- crate-root lint attributes from `crate_attrs`.

Error-handling etiquettes share one source scan through `error_ir`.

Use `--deny-open` in CI when a report should still be written but unresolved
action items from `quality-report.md` should fail the job. Inventory-only
`Disposition::Open` findings, exemplars, and suppressed findings remain report
evidence; they do not trip the deny gate.

## Promoting `dev` to `main`

CI does not run on ordinary pushes to `dev`. A ruleset on `main` requires a
pull request and green checks before the fast-forward lands:

1. `check-all (ubuntu-latest)` — `just check-all` (`fmt`, `clippy -D warnings`,
   `test --features full`).
2. `check-features (ubuntu-latest)` — `just check-features`
   (`cargo hack` feature powerset, depth 2).
3. `cordial-gate (ubuntu-latest)` — `just cordial-gate`
   (`cordial quality --deny-open` from this checkout).

Pre-flight from `dev` without opening a PR:

```sh
just check-all
just check-features
just cordial-gate
gh workflow run ci.yml --ref dev
```

`main` also requires linear history (rebase or squash). Repository admins can
bypass the PR + CI ruleset; history rewrite stays blocked.

## Coverage

Coverage etiquettes read rustdoc JSON from the store
(`~/.cordial/{project}/cache/rustdoc/`). Workspace-member rustdoc cache inputs
are rebuilt automatically when missing or stale:

```sh
cordial coverage
```

Use the explicit build command when you want to prewarm or force-refresh that
cache:

```sh
cordial build rustdoc
```

Std-family coverage also needs sysroot rustdoc:

```sh
cordial build sysroot
cordial coverage
```

The default `full` install includes `elicitation` coverage
(`impl-coverage`, `trenchcoat`, `shadow`), `homecoming_std`, and
`amenable_std`.

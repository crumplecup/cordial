# `feature_warnings` etiquette

Status: **Active**

`cargo check`, clippy, and `just check-all` compile one feature set. A
warning that only fires under another combination (an import used only by
code behind `feature = "x"`, a helper whose one consumer is gated off) is
invisible there. `just check-features` compiles the powerset, but
`cargo hack check` only *fails* on errors, so warnings scroll past. The
first full sweep of cordial found about 98 distinct sites in 21 files,
reported by 77 of 78 feature combinations.

This etiquette runs the powerset, treats every feature-dependent warning as
an action item, and says how to fix each one. It is the post-process
counterpart of [doc_warnings](doc-warnings-etiquette.md): rustc's own
output, not a source scan, because whether an item is used under a
combination needs `cfg` evaluation and name resolution.

Agents must close open items before merging to `main`
(`cordial quality --deny-open`), which makes these warnings a merge gate.

---

## What counts

| Shape | Flagged |
| --- | --- |
| A rustc warning that fires in some feature combinations and is absent in others | yes, one finding per `(file, line, lint)` |
| A warning that fires in **every** combination | no: an ordinary warning; `cargo check` and clippy already see it (`include_universal = true` flips this) |
| Combinations that fail to compile | no: errors belong to `cargo check` |
| `cargo-hack` or `cargo` missing from `PATH` | skip the crate; quality must still run |
| package in `[feature_warnings] skip_crates` | no |

Rules:

| Id | Lint family | Meaning |
| --- | --- | --- |
| `FEATURE-WARNING-001` | `unused_*` (imports, variables, mut, macros, ...) | Something is imported or bound but unused under some combinations |
| `FEATURE-WARNING-002` | `dead_code` | A function, constant, struct, or method is never used under some combinations |

---

## Invocation

```text
cargo hack check --feature-powerset --depth <n> --keep-going \
    --message-format=json -p <crate> --target-dir <crate>/target/feature-warnings
```

plus `--exclude-features` and `--group-features` from config.
`CARGO_INCREMENTAL=0` (every combination is a cold cache; see the note on
`check-features` in the `justfile`). `--no-dev-deps` is **not** passed: it
rewrites `Cargo.toml` while running, which an analysis tool must not do.

## Attributing a warning to a combination

`compiler-message` records carry no feature set, but each package's
`compiler-artifact` record does (`features`: the resolved set, implied
features included). Messages for a package are buffered until its next
artifact and tagged with that artifact's features. The distinct feature
sets seen for the package are the combinations run; a site's *triggering*
combinations are those whose messages include it, its *silent* combinations
the rest.

Dedup key is `(file, line, lint code)`. Unused-import messages list
different names per combination, so the message shown is the union of the
backticked names.

## Actionable advice

Given the triggering set `T` and silent set `S` of combinations:

1. `E` = features that appear in some silent combination and in no
   triggering one. If every silent combination contains a feature of `E`,
   the fix is `#[cfg(any(feature = "...", ...))]` over `E`.
2. Otherwise let `I` be the features common to every silent combination. If
   no triggering combination contains all of `I`, the fix is
   `#[cfg(all(feature = "...", ...))]` over `I`.
3. Otherwise no single gate fits: share the item through an internal
   feature, or move it next to its only consumer.

The checklist groups items by suggested gate, so one gate edit clears a
whole cluster. Per-lint wording:

| Lint family | Advice |
| --- | --- |
| `unused_imports` | Gate the `use` with the suggested `cfg`, or move it into the gated module that uses it |
| `dead_code` | Gate the definition the same way, or move it into the file of its only consumer |
| `unused_variables`, `unused_mut` | The binding is used or mutated only under the suggested features; gate the statement or the `mut` |
| other | Gate with the suggested `cfg` |

Project preference (from the cleanup that motivated this): move a helper
next to its single consumer when there is one, and reach for a shared
private feature only when several etiquettes use it.

### Private features

A feature whose name starts with `_` is **private**: only other features
enable it, a user never does. When a gate would name more than
`private_feature_threshold` features (default 6), listing them is not a
readable fix. The advice is to add a private feature (`_<area>_support`)
that each of them enables, and gate the item on it. The checklist shows such
items under "Needs a private feature" with the count and the first few
names; the CSV keeps the full predicate. After the fix lands, a rerun sees
the single private feature as the gate and the warnings are gone.

The powerset excludes `_` features as units automatically (read from
`cargo metadata`), so they neither multiply the combinations nor need hand
maintained `exclude_features` entries. They still appear in the resolved
feature sets, so gate advice can name them.

Gate advice also (a) removes `default` and the configured
`exclude_features` umbrellas before computing, because a `cfg` should not
name `full` or `quality`; and (b) counts a silent combination as evidence
only when the item is compiled there, approximated as "the combination
contains every feature common to the triggering ones". Without (b), an item
that is gated out of most combinations reads as widely used.

---

## Config

```toml
[feature_warnings]
# enabled = false              # off by default: minutes of cold checks
# depth = 2                    # powerset depth
# exclude_features = ["slow_tests", "full", "quality", "elicitation"]
# group_features = [["allows", "modularity"], ["rustdoc", "shadow"]]
# private_feature_threshold = 6   # wider gates advise a private feature
# include_universal = false
# skip_crates = []
```

Off by default because the powerset is slow; cordial's own `cordial.toml`
turns it on so the merge gate covers it. `group_features` mirrors the
`justfile`'s `--group-features`. The two lists can drift; moving
`check-features` to read this table is a follow-up.

## Outputs

`{store}/findings/feature-warnings.checklist.md`,
`feature-warnings-summary.md`, and `feature-warnings.csv`
(`crate,rule_id,lint,file,line,gate,triggering,silent,message,advice`).

## Cost and caching

The enricher output lives in the cached IR, keyed on source digests, so an
unchanged tree re-reports without recompiling. A `Cargo.toml` feature edit
does not invalidate that cache; rerun with `--no-cache`.

## Phases

| Phase | Work | Status |
| --- | --- | --- |
| 1 | Parse cargo-hack JSON, attribute to combinations, compute gate advice (pure functions, tested on synthetic output) | **Done** |
| 2 | Etiquette bundle: config, enricher, probe, assessor, reporters, registration, explain | **Done** |
| 3 | Dogfood on cordial; fix the warnings it reports | Not started |
| 4 | `check-features` reads `[feature_warnings]`; drop the duplicated flag lists | Not started |

## Open questions

- Whether artifact `features` (resolved, implied included) or the requested
  features gives better gate advice. Resolved is what `cfg(feature = ...)`
  sees, so it is the default.
- Workspaces: one `cargo hack` per member via `-p`, run from the workspace
  root (diagnostic paths are workspace-relative, as with `doc_warnings`).

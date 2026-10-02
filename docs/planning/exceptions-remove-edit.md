# `cordial exceptions remove` / `edit` and stale-exception warnings

Status: **Active**

## Problem

`cordial exceptions` could `add` rows but never change or delete one. A
refactor that renamed a module made two exceptions' `context` strings stop
matching (`ExceptionEntry::matches` compares `context` exactly), and nothing in
cordial's output said the exceptions had gone inert -- the findings simply
re-opened. Reported by the amenable agent using cordial as a gate.

## Design

### Selector

An `ExceptionSelector` names a row by any combination of `rule_id`,
`context`, `file`, and `line`. Each given field must equal the stored value
exactly (file paths are normalized the same way `add` normalizes them). At
least one field is required. A selector matching zero rows or more than one
row is an error; nothing is changed. The search covers the canonical file
(`{store}/exceptions/{etiquette}/{crate}.json`) and the elicit_doc alias
(`{store}/quality/patches/...`), and the row is rewritten in whichever file
holds it.

### `remove`

`remove_exception(store, etiquette, crate, selector)` deletes exactly one
row. When it was the last row the file is deleted (and its etiquette directory
when that becomes empty), so there is no empty-array file for `backup` to copy.
`backup` already replaces the whole subtree, so a re-run after `remove` leaves
no stale copy in the registry.

### `edit`

`update_exception(store, etiquette, crate, selector, update)` replaces one row
in place. `ExceptionUpdate` carries optional new `file`, `line`, `rule_id`,
`context`, and `reason`; unset fields keep their value. The result goes through
the same normalization as `add`, and an edit that would duplicate another row is
an error.

### Stale warnings

`stale_exceptions(findings, sets)` returns entries whose category produced no
matching finding in this run. The session warns once per stale entry after
exceptions are applied, naming the crate, etiquette, selector fields, and
reason. It is a warning, not a gate: an etiquette that was skipped or a crate
outside the run filter is never reported (sets are only loaded for the
selected etiquettes and crates).

A dedicated `list --stale` is not provided; the run-time warning sees the same
findings and needs no second analysis pass.

## CLI

```sh
cordial exceptions remove <etiquette> --rule-id R [--context C] [--file F] [--line N] [--crate-name X]
cordial exceptions edit <etiquette> --context C [--rule-id R] ... \
    [--new-context C2] [--new-file F2] [--new-line N] [--new-rule-id R2] [--new-reason TEXT]
```

## Status

- [x] Design
- [x] Library API: `remove_exception`, `update_exception`
- [x] CLI `remove` / `edit`
- [x] Stale warning in the session run
- [x] Tests in `tests/exceptions.rs`, `tests/cli.rs`

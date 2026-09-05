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
cordial quality -p <project>          # source-quality etiquettes
cordial quality --apply               # tracing recipes + crate-root lint attributes
cordial quality --apply --dry-run     # log apply without writing
cordial explain                       # id + one-line why for every compiled etiquette
cordial explain doc_warnings          # why it exists, what it flags, how to opt out
cordial explain DOC-WARNING-001       # same page (rule id alias)
cordial build rustdoc                 # rustdoc JSON for coverage
cordial build sysroot                 # std-family rustdoc for framework coverage
cordial coverage                      # impl / trenchcoat / shadow / std-family coverage
cordial run                           # quality + coverage
cordial view findings/quality-report.md
```

`-p` / `CORDIAL_PROJECT` selects the project root. `--crate-name` restricts a
run to one crate. `--store-home` / `CORDIAL_HOME` overrides `~/.cordial`.

## Store

Reports write to `{store_home}/{project}/findings/`:

- CSV inventories;
- markdown checklists;
- markdown summaries;
- `findings/quality-report.md` for the quality rollup.

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

## Quality

`cordial quality` runs every source-quality etiquette compiled into the binary.
Quality `--apply` rewrites source only for supported apply paths:

- tracing instrument recipes from the checklist;
- crate-root lint attributes from `crate_attrs`.

Error-handling etiquettes share one source scan through `error_ir`.

## Coverage

Coverage etiquettes need rustdoc JSON:

```sh
cordial build rustdoc
cordial coverage
```

Std-family coverage also needs sysroot rustdoc:

```sh
cordial build sysroot
cordial coverage
```

The default `full` install includes `elicitation` coverage
(`impl-coverage`, `trenchcoat`, `shadow`), `homecoming_std`, and
`amenable_std`.

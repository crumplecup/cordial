# `cordial.toml` config

Canonical home for etiquette thresholds. Loaded with the [`config`](https://docs.rs/config)
crate — do not hand-roll TOML/JSON readers for these knobs.

## Files

Later sources win:

1. [`CordialConfig::default`](../../src/config.rs) — graceful fallback
2. `{store_home}/cordial.toml` — `~/.cordial` unless `CORDIAL_HOME` / `--store-home`
3. `{workspace}/cordial.toml` — project root (highest priority)

Missing or unreadable files do not fail the run; the `Default` impl is the
backup plan. Drift between `Default` and a committed `cordial.toml` is
accepted.

## Shared lint policy

Every built-in and plugin etiquette supports `[lints.<etiquette-id>]`, using
its `TomlConfigured::configuration_key()`. No plugin-specific config registration
is necessary. Existing built-in `[tracing] enabled = false` switches remain
supported; the shared `enabled` switch takes precedence when specified.
Strategy and threshold settings stay in their existing etiquette tables.

```toml
[lints.tracing]
enabled = true
rules = { "TRACING-STD-PRINTLN" = false }
skip_paths = { "TRACING-MISSING-INSTRUMENT" = ["src/verus_witness"] }

[[lints.tracing.paths]]
path = "src/generated"
enabled = false

[[lints.tracing.paths]]
path = "src/generated/handwritten.rs"
rules = { "TRACING-MISSING-INSTRUMENT" = true }

[[lints.tracing.paths]]
path = "crates/proofs/src"
base = "workspace" # default: "crate"
crate = "proofs"    # optional Cargo crate-name filter
enabled = false

[lints.acme-todo.rules] # custom plugin etiquette
"ACME-TODO-001" = false
```

Defaults enable all checks. Resolve the etiquette switch, then the rule switch,
then matching paths from broadest to narrowest. At each path, rule switches
override `enabled`; equal paths use declaration order, with the last winning.
A narrower enable can restore a globally disabled check. `skip_paths` applies
before explicit path entries, which can restore a skipped rule.

Paths match a complete file or folder component prefix, without globs. `src/a`
matches `src/a/code.rs`, but not `src/another.rs`. Paths are relative to the
crate root unless `base = "workspace"` is specified. Absolute configured paths
and paths containing `..` are not matched. Slashes and backslashes are accepted.
Findings with no file evidence receive only global controls. A workspace finding
with an absolute file can receive workspace path policy; crate-relative policy
requires its crate identity.

The session removes disabled findings before exceptions, reports, gate accounting,
and worklist generation. Apply uses the same controls, including when reading
an old checklist. A new tracing annotation uses its complete safe recipe; repairs
to an existing annotation honor each enabled rule. An atomic error conversion is
left unchanged when a required field/source/Kind rule is disabled anywhere in its
connected file graph. Existing `[antipatterns.skip_paths]` remains supported.
Configuration switches are policy choices; they do not create approved exceptions.

## Strategy and threshold examples

```toml
[tiers]
# default = ["build"]  # also runs quick checks
# `--tier all` runs every known tier, including `deep`.

[tiers.assign]
# dependency_freshness = "ci"  # project assignment overrides the built-in tier

[worklist]
# max_passes = 3       # tell the user when an acted-on item reaches this list count

[visibility]
# enabled = true
max_crate_names_for_flat = 50
min_module_names = 10
prefer_root = true

[modularity]
# enabled = true
file_inventory_min_lines = 500
function_inventory_min_lines = 150
function_hotspot_min_lines = 80
file_checklist_min_lines = 500
function_checklist_min_lines = 200
max_types_per_file = 10
module_size_sigma = 2
module_size_ignore_lower_tail = false
min_module_lines = 0
top_heavy_min_percent = 50
lopsided_min_percent = 75
hierarchy_min_lines = 150
# generated_files = ["src/derived_witness", "src/some_generated_file.rs"]

[cfg_scatter]
# enabled = true
min_distinct_kinds = 2
min_occurrences = 5

[crate_attrs]
# enabled = true
# Both default true. Per-member exceptions keep an FFI crate free to use
# unsafe without turning the lint off workspace-wide.
# forbid_unsafe = true
# missing_docs = true
# allow_unsafe = ["ffi"]
# allow_missing_docs = []

[doc_warnings]
# enabled = true
# document_private_items = false
# all_features = false
# skip_crates = []

[feature_warnings]
# enabled = true          # selected by the deep tier
# depth = 2
# exclude_features = []
# group_features = []
# private_feature_threshold = 6
# include_universal = false
# skip_crates = []

[dependency_freshness]
# enabled = true
# patch = true
# minor = true
# major = true
# manifest_exact_pin = true
# manifest_upper_bound = true
# manifest_wildcard = true
# manifest_tilde = true
# manifest_workspace_bypass = true

[derives]
# enabled = true
max_constructor_args = 3
min_fluent_setters = 2

[pageantry]
# enabled = true
# trait_block = true
# barrel = true
# barrel_shim = true
# max_shim_lines = 8

[antipatterns]
# enabled = true

[antipatterns.static_refs]
# default: "string"; accepted: "string", "cow", "const"
# "const" is contextual: const/static-only types stay quiet; runtime or
# unclear construction falls back to Cow<'static, str>.
strategy = "string"

[tracing]
# enabled = true
# extra_skip = ["inventory"]
# apply_gate_crates = { amenable_kani = "kani" }
# apply_skip_crates = ["amenable_verus", "amenable_creusot"]

[tracing.subscriber]
# All default true.
# init_in_main = true
# init_in_tests = true
# helper_in_lib = true
# rust_log_fallback = true
# idempotent = true

[tracing.stdio]
# All default true. One leftover-stdio lint per macro.
# println = true
# eprintln = true
# print = true
# eprint = true
# dbg = true
# skip_cargo_protocol = true
# skip_folders = ["tests/fixtures", "tests/parity"]
```

`apply_gate_crates` wraps `--apply`'s `#[instrument]` as
`#[cfg_attr(not(<cfg>), tracing::instrument(...))]` for that crate and
anything that compiles under the same graph-wide flag (transitive Cargo
dependents, plus `#[path]` splice consumers). `apply_skip_crates` never
writes `#[instrument]` (Verus bare compiler / Creusot translator);
skip does not follow ordinary deps, but does follow a `#[path]` splice.

Etiquettes with no other knobs still have a table so they can be turned
off: `[panics] enabled = false`, `[pageantry] enabled = false`,
`[impl-coverage] enabled = false`, and so on. Custom plugins with no
table stay on.

Add a table per etiquette as new knobs appear. Tracing role→level maps stay
in code until dogfood needs a project override.

## Status

Implemented (`tests/cordial_config_*.rs`). Every etiquette table has
`enabled` (default true); `CordialConfig::etiquette_enabled` gates the
session. Visibility, modularity (including types-per-file, module-size
2σ, lower-tail ignore, hotspot diagnosis, hierarchy lints: top-heavy,
lopsided, unary-nest collapse, and a `generated_files` exceptions list
for the file-size and module-size checks), cfg_scatter, derives
(`max_constructor_args`, `min_fluent_setters`), tracing (`extra_skip`,
`apply_gate_crates`, `apply_skip_crates`, nested `[tracing.subscriber]`
and `[tracing.stdio]`),
crate_attrs (`forbid_unsafe`, `missing_docs`, `allow_unsafe`,
`allow_missing_docs`), and doc_warnings (`document_private_items`,
`all_features`, `skip_crates`), creusot_diagnostics (`skip_crates`),
dependency_freshness (`enabled`, `patch`, `minor`, `major`,
`manifest_exact_pin`, `manifest_upper_bound`, `manifest_wildcard`,
`manifest_tilde`, `manifest_workspace_bypass`), and antipatterns
(`static_refs.strategy`) read through `load_session_config`.
Role→level maps stay in code.

```toml
[creusot_diagnostics]
enabled = true
skip_crates = ["legacy_creusot"]
```

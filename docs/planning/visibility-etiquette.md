# Visibility etiquette

Static lint for **module visibility topology**: a `pub mod` (or `pub(crate)
mod`) has to earn its path. Companion to [cfg-scatter](cfg-scatter-etiquette.md)
(feature-gate scatter) and `modularity` (file/function size). Pub *fields* stay
in the derives etiquette (`DERIVE-PUB-FIELD-001`).

## Rules

Thresholds are **not** compiled into the scanner. Pass them on the call or
load `cordial.toml` (workspace, then `~/.cordial`, then `CordialConfig::default`):

```rust
scan_crate_visibility(crate_root, VisibilityThresholds::default())?;
scan_crate_visibility(crate_root, load_visibility_thresholds(workspace, store_home))?;
```

```toml
# {workspace}/cordial.toml  (wins)
# {store_home}/cordial.toml  (~/.cordial)
[visibility]
max_crate_names_for_flat = 50
min_module_names = 10
prefer_root = true
```

See [cordial-config.md](cordial-config.md). Session enricher uses
`load_session_config`.

When every path module is under `min_module_names` and flattening them would
leave more than `max_crate_names_for_flat` names at root, the two knobs
conflict. **`prefer_root` (default true)** keeps the fat root: no extra lint
for a 68-name `lib.rs`. Undersized modules still flatten via
`VIS-MOD-THIN-001`.

Set `"prefer_root": false` to peel the largest undersized public modules
until remaining root is under the max (e.g. keep 9, 7, 7, 6). The scanner
enters **branching** mode: a floor that starts at `min_module_names` and
drops with each peel, so those kept modules do not fire thin. The floor is
cached under `{store}/cache/{crate}-visibility-branching.json` keyed by a
source digest; a code change invalidates it and re-peels. That is a
two-pass analysis (peel, then apply the lowered floor), not a permanent
exception.

| Rule | When |
| --- | --- |
| `VIS-CRATE-FLAT-001` | Externally reachable `pub` names `< max_crate_names_for_flat`, but a `pub mod` sits on a fully public path. Keep `mod` private and re-export at `lib.rs`. |
| `VIS-MOD-THIN-001` | A visible module (`pub`, `pub(crate)`, or `pub` under a non-`pub` parent) has fewer leaf names than `min_module_names`. |
| `VIS-MOD-MISMATCH-001` | Child is unrestricted `pub` while the parent is not — the agent-hostile `pub mod` inside a private mod. Spell `pub(crate)` if you meant crate-wide and the module meets the floor; otherwise private child + parent `pub use`. |

Leaf names: `pub`/`pub(crate)` structs, enums, traits, fns, types, consts,
statics, and `pub use` names. Not fields, not the module node itself,
not `#[doc(hidden)]` / `#[cfg(test)]` modules.

## Dogfood results

First self-scan of the cordial workspace (`quality`, thresholds 50 / 10)
produced **39** findings: 24 `VIS-MOD-MISMATCH-001` (`pub mod` under a
private parent), 14 `VIS-MOD-THIN-001`, and 1 `VIS-CRATE-FLAT-001`
(`cordial_elicitation::tracked_targets`).

Fix pattern, same as the lint's recommended spelling:

1. **Private child + parent `pub use`** — used for hook trait modules
   (`src/hooks/{assessor,enricher,…}`), `digest`, `rustdoc::fixture`, and
   `cordial_elicitation::tracked_targets`. The path disappears; callers
   import from the parent (`cordial::rustdoc::demo_shadow_crate`,
   `crate::hooks::Assessor`).
2. **`pub(crate) mod` instead of `pub mod`** — used for etiquette packages
   under private `src/etiquettes`. That kills mismatch (the child vis now
   matches crate-internal intent) without flattening the family namespace.

### Reporter fix: checklist/summary used the primary IR crate

Workspace findings already carry a per-finding `crate` field (CSV was
correct), but the checklist heading and summary table used
`ir.crate_name()` — the primary crate — so `cordial_elicitation` rows
showed up under `cordial`. Grouping now follows the finding field.

### Flattened: `error_ir` is not an etiquette

`crate::etiquettes::error_ir` had two leaf names (`scan_rust_file_syntax`,
`ErrorIrScanLayers`). It is a shared scanner, not a hook bundle, so the
module is now private with those two names re-exported at
`crate::etiquettes`.

### Accepted residuals: thin `pub(crate)` etiquette packages

Four packages stay under the floor of 10 because the extra path component
namespaces a family (`scan_rust_source`, assessor/probe/reporter types)
that sibling crates import as `crate::etiquettes::<family>::…`. Flattening
them onto `etiquettes/mod.rs` would collide (`scan_rust_source` is
repeated) or dump a kitchen-sink. Left as-is:

| Module | Names |
| --- | ---: |
| `crate::etiquettes::derives` | 9 |
| `crate::etiquettes::impl_coverage` | 9 |
| `crate::etiquettes::shadow` | 8 |
| `crate::etiquettes::trenchcoat` | 4 |

Project config: `{repo}/cordial.toml` pins the dogfood knobs under
`[visibility]`.

## Status

Implemented (`tests/visibility_etiquette.rs`). Feature `visibility`, folded
into `quality`. First dogfood pass complete: mismatches and crate-flat
cleared; four thin etiquette-package residuals remain (see above).
`prefer_root` / branching peel is implemented; this workspace does not hit
the all-thin-vs-fat-root conflict.

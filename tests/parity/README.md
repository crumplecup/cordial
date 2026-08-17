# Parity fixtures

Tier A workspaces and frozen `elicit_doc` CSV baselines for output parity tests.

## Re-freeze baselines

After changing a workspace fixture or upgrading `elicit_doc`:

```bash
export CARGO_TARGET_DIR=/path/to/elicit_doc/target
BASE="$PWD/tests/parity/baseline"
OUT="$PWD/tests/parity/.elicit-out"

for ws in panic_sources simple_fn mixed_visibilities; do
  WS="$PWD/tests/parity/workspaces/$ws"
  mkdir -p "$OUT/$ws" "$BASE/$ws/findings"
  (cd /path/to/elicit_doc && cargo run -q -- quality panics \
    --project "$WS" --output-dir "$OUT/$ws")
  (cd /path/to/elicit_doc && cargo run -q -- quality tracing \
    --project "$WS" --output-dir "$OUT/$ws")
  cp "$OUT/$ws/panics.csv" "$BASE/$ws/findings/"
  cp "$OUT/$ws/tracing-instrument.csv" "$BASE/$ws/findings/"
done

cargo test --features quality --test parity
```

Workspace fixture `Cargo.toml` files include an empty `[workspace]` table so they
are standalone crates (not members of the cordial workspace).

## Run parity tests

```bash
cargo test --features quality --test parity
```

## Tier C coverage parity (homecoming / amenable)

Live dual-run against sibling `elicit_doc` and framework workspace checkouts.
Not run in default CI (ignored tests; requires `PARITY_TIER_C=1`).

```bash
cordial build sysroot   # once per nightly toolchain

export PARITY_TIER_C=1
export PARITY_HOMECOMING_ROOT=/path/to/homecoming   # optional
export PARITY_AMENABLE_ROOT=/path/to/amenable
export ELICIT_DOC_MANIFEST=/path/to/elicit_doc/Cargo.toml

cargo test --features full --test coverage_parity -- --ignored --nocapture
cargo test --features full --test coverage_parity_refresh -- --ignored --nocapture
```

Frozen gap baselines: `tests/parity/baseline/tier_c/{homecoming,amenable}/findings/gaps-impl.csv`.

## Tier A elicitation coverage parity (minimal-workspace)

Url-scoped impl coverage (`gaps-impl.csv`) vs frozen baseline from `elicit_doc`'s pipeline fixture.

```bash
cargo test --features impl_coverage --test elicitation_parity

# Re-freeze after cordial or elicit_doc gap logic changes:
cargo test --features impl_coverage --test elicitation_parity_refresh \
  refresh_minimal_workspace_gaps_impl_baseline -- --ignored --nocapture
```

Baseline: `tests/parity/baseline/minimal-workspace/findings/gaps-impl.csv`.
Requires dev-dep path to `elicit_doc` for the refresh test only.

Feature probe rustdoc (optional live rebuild): set `CORDIAL_PROBE_FEATURES=1` or seed
`{store}/cache/rustdoc-probe/{crate}.json` before running impl coverage.

## Tier C quality parity (amenable antipatterns)

Live dual-run: `elicit_doc quality antipatterns` vs `cordial` antipatterns etiquette on the amenable workspace.

```bash
export PARITY_TIER_C=1
export PARITY_AMENABLE_ROOT=/path/to/amenable
export ELICIT_DOC_MANIFEST=/path/to/elicit_doc/Cargo.toml

cargo test --features full --test quality_parity -- --ignored --nocapture
```

Requires `amenable` on PATH (registry dump for contract-bounds rule). Compares open-finding recall + precision on `antipatterns.csv` and `version-in-member.csv`.

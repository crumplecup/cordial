# Cursor's sandbox intercepts execve so rustup shims see argv[0]="cursor" and
# break. Point at the real toolchain binaries when they exist so cargo and rustc
# are the stable release — not the sandbox intercept.
# Clear CARGO_TARGET_DIR so builds use this workspace's target/ rather than a
# sandbox cache compiled by a different rustc.
cargo := if path_exists(home_directory() / ".rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin/cargo") == "true" {
    home_directory() / ".rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin/cargo"
} else {
    "cargo"
}

rustc := if path_exists(home_directory() / ".rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin/rustc") == "true" {
    home_directory() / ".rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin/rustc"
} else {
    "rustc"
}

export RUSTC := rustc
export CARGO_TARGET_DIR := justfile_directory() / "target"

# Features baked into the installed binary. `full` is the workstation default
# (quality + coverage + std-family). Slim quality-only:
#   just install features=quality,cli
features := "full"

default:
    just --list

# Install the release binary to ~/.cargo/bin (`cordial` on PATH).
install:
    {{cargo}} install --path {{justfile_directory()}} --bin cordial --force --locked --profile release --features {{features}}

# Routine test run through cargo-nextest (`cargo install cargo-nextest`), which
# runs tests from all binaries in parallel. The default `quick` feature is on,
# so tests over 60s are not compiled, and `--all-features` is quick-safe too
# since it enables `quick`. nextest does not run doctests, so they run after.
test:
    {{cargo}} nextest run --features {{features}}
    {{cargo}} test --doc --features {{features}}

# Full run including the slow tests (over 60s each; real database loads and
# dogfood scans -- minutes, not seconds). Features cannot subtract, so this
# drops the defaults (and with them `quick`) instead of adding a feature.
# Run deliberately, not as part of routine verification.
test-slow:
    {{cargo}} nextest run --profile slow --no-default-features --features {{features}}
    {{cargo}} test --doc --no-default-features --features {{features}}

fmt:
    {{cargo}} fmt --all --check

clippy:
    {{cargo}} clippy --all-targets --features {{features}} -- -D warnings

# Pre-merge rustc gates. CI runs this as `check-all (ubuntu-latest)`.
check-all:
    just fmt
    just clippy
    just test

# Feature-powerset compile check. Depth 2: each unit and each pair.
# Umbrellas (`full`, `quality`, `elicitation`) are already compiled by
# `just check-all`. `quick` is excluded. Independent etiquette flags
# that do not pull crates are grouped so the pair count stays runner-
# sized (~80 `cargo check`s, not C(30, 2)). `--no-dev-deps` temporarily
# edits Cargo.toml (restored on exit). Needs cargo-hack.
# CI runs this as `check-features (ubuntu-latest)`.
#
# CARGO_INCREMENTAL=0: every one of these ~80 checks uses a different
# feature set, and incremental's cache key includes the active features,
# so every single check is a cold cache anyway -- incremental buys no
# reuse here, only pays its memory/bookkeeping overhead ~80 times over
# and leaves an orphaned incremental dir per combo if a run gets killed
# before cleanup (confirmed: with it on, a kill left ~250 stale
# incremental dirs totaling several GB; with it off, the full sweep ran
# clean with host memory never dropping below 19GB free).
check-features:
    CARGO_INCREMENTAL=0 {{cargo}} hack check \
        --feature-powerset \
        --depth 2 \
        --no-dev-deps \
        --keep-going \
        --exclude-features quick,full,quality,elicitation,hotpath,_quality_support,_attribute_scan,_etiquette_plugins \
        --group-features allows,modularity,derives,cfg_scatter,visibility,cli_layout,doc_warnings,feature_warnings,glob_imports,inline_tests,pageantry,verus_warnings,creusot_diagnostics \
        --group-features error_sites,error_chain,internal_error_chain,foreign_error_types,foreign_error_attenuation \
        --group-features rustdoc,impl_coverage,trenchcoat,shadow,homecoming_std,amenable_std,amenable_ext

# Quality-etiquette gate: write reports, then fail if `quality-report.md`
# still has open action items. Uses this checkout's binary, not a pinned
# install. CI runs this as `cordial-gate (ubuntu-latest)`.
#
# CI and promotion only. It runs every tier, including the deep feature
# powerset (about twenty minutes), and `exceptions load` replaces the local
# exception store from the repo files. Local development and agents work
# through `cordial open` / `cordial submit` (default build tier) instead.
cordial-gate:
    {{cargo}} run --features {{features}} --bin cordial -- exceptions load
    {{cargo}} run --features {{features}} --bin cordial -- quality --deny-open --tier all

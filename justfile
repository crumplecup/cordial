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
    {{cargo}} install --path {{justfile_directory()}} --bin cordial --force --locked --features {{features}}

# Routine test run. Deliberately `--features full`, not `--all-features`:
# `--all-features` unconditionally enables `slow_tests` too (inherent to
# what --all-features means -- no feature declaration can opt out of it),
# which pulls in tests too expensive/environment-dependent for routine
# verification (see Cargo.toml's own comment on that feature).
test:
    {{cargo}} test --features {{features}}

# Full run including `slow_tests` -- real subprocess/heavy-parse tests,
# minutes not seconds. Run deliberately, not as part of routine
# verification.
test-slow:
    {{cargo}} test --features {{features}},slow_tests

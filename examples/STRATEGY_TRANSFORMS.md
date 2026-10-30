# Executable strategy specimens

These Cargo examples are intentional transformation inputs, not recommended
error-handling patterns. They contain no lint suppressions or exceptions.
Run one with `cargo run --example apply_panic_result -- fail`.

| Cargo example | Isolated obligation | Baseline behavior |
| --- | --- | --- |
| `apply_return_lifting` | Wrap both explicit early returns and the tail when lifting a return type | Prints `11`, or `7` with `early`. |
| `apply_return_scopes` | Separate nested returns and identify fixed signatures and residual propagation | Prints `10 1 Ok(8)`, or `2 1 Ok(8)` with `early`. |
| `apply_return_paths` | Lift branch, match, nested return, and unit exits without changing evaluation counts | Exercises all paths; counted calls return `11` and `112`, leaving counter `112`. |
| `apply_panic_result` | Construct a typed error chain from a fresh Result panic site, one caller, and main | Prints `8`; panics with `fail`, retaining `PermissionDenied` in its diagnostic. |
| `apply_failure_site` | Propagate one resolved Result failure inside an already fallible function | Prints `Ok(8)`; initially panics with `fail`. |
| `apply_unwrap_lookalike` | Refuse rewriting a user-defined same-spelling method as Result propagation | Prints `7`; its `unwrap` returns `u32`. |

The integration harness in `tests/strategy_specimens.rs` copies each example's
actual `main.rs` bytes into an isolated Cargo package at `src/main.rs`, with an
explicit binary target. The manifest and canonical source-root mapping are
part of the fixture contract. No second inline copy of the program is kept.
The tests compile and execute that binary, run the production pipeline, and
inspect its saved shared IR. The checked-in examples remain unchanged.

Cordial's current source loader discovers `src/`; quality scanners discover
`src/` and `tests/`. They do not discover Cargo example targets outside those
roots. Normalizing each specimen into a binary package makes it visible to the
ordinary loader, rather than claiming support for direct example-target
migration. The harness asserts both Cargo's binary discovery and the expected
function and panic-site inventory, preventing an empty scan from passing.
Changing general Cargo target discovery remains a separate capability.

`tests/strategy_return_lift.rs` now exercises the production isolated return
lifting planner, compiles its rendered output, and executes both successful
paths. Its Debug boundary observes the same values inside `Ok`. Preview does
not write; the harness writes only its disposable copy. Recollection proves
repeat planning adds no wrappers. Fixed boundaries and stale or corrupt
evidence are refused. This does not yet prove a complete panic migration.

`tests/strategy_return_paths.rs` extends that contract to structural exits. Each
function is lifted independently through the production planner, compiled
without warnings, and executed on every path. Full output comparison checks
that other functions and side effects remain unchanged. Collection retains the
structural exit positions in the shared IR; apply never rediscovers syntax.

In particular,
the existing panic inventory also flags the lookalike's method call: a finding
based on spelling must never authorize an apply edit. Receiver resolution is
required before failure-site rewriting can be implemented.

`tests/strategy_result_contracts.rs` checks absolute standard Result declarations,
source type arguments, located direct calls, and unwrap receiver ranges through
the production loader and persisted IR. Relative paths, aliases, generic errors,
and shadowed call names retain unknown evidence. Nested bodies have separate
execution scopes. These facts do not yet authorize edits or prove caller closure.

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
| `apply_result_propagation` | Preserve source evaluation and side-effect order while propagating a resolved failure | Prints `Ok(8)` with a trace; initially panics with `fail`. |
| `apply_error_declaration` | Construct an owned parent / Kind / native source chain from a resolved I/O failure | Prints `8`; initially panics with `fail`. |
| `apply_error_return` | Lift early and tail successes to a generated error contract independently of failure rewriting | Prints `8`, or `11` with `early`; initially panics with `fail`. |
| `apply_error_caller` | Adapt every call from one caller to a newly fallible callee, including exact-tail and early-return calls | Prints `14 2`, or `7 1` with `early` or `tail`; initially panics with `fail`. |
| `apply_error_component` | Compose a fresh typed error migration through a multilevel caller graph, diamond, repeated sites, and self/mutual recursion | Prints `24 3`, or `25 3` with `deep`; initially panics with `fail`. |
| `apply_function_uses` | Inventory direct calls, function values, qualified paths, nested scopes, and macro boundaries | Prints `45`. |
| `apply_error_main` | Close the transitive migration through Cargo's unit `main` and returned-error termination | Prints `success`; initially panics with `fail`. |

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
required to authorize failure-site rewriting.

`tests/strategy_result_contracts.rs` checks absolute standard Result declarations,
source type arguments, located direct calls, and unwrap receiver ranges through
the production loader and persisted IR. Relative paths, aliases, generic errors,
and shadowed call names retain unknown evidence. Nested bodies have separate
execution scopes. These facts do not yet authorize edits or prove caller closure.

`tests/strategy_call_resolution.rs` exercises the IR enrichment that joins direct
calls to same-module source declarations. Located records retain snapshot-bound
callee identities, while `Calls` edges expose incoming callers. Repeated sites
and recursion retain their identities; shadowed names and opaque namespaces stay
unresolved. Missing edges cannot be interpreted as proof of complete caller
coverage. Broader type inference, imports, and cross-module calls remain open.

`apply_result_propagation` exercises an isolated Result `unwrap` to `?` operation
inside an already fallible function. Its integration test renders the production
IR-backed preview, compiles both forms, and checks successful values, evaluation
order, comments, and a returned typed source error. Additional tests refuse
lookalike methods and unresolved/error-mismatched contracts, reject stale or
corrupt evidence, and compile independent repeated-site previews after reload.
This operation does not yet provide a complete panic-to-error CLI migration.

`tests/strategy_error_declaration.rs` generates a fresh owned error architecture
through the production declaration planner. A surrounding test consumer proves
that the generated `From` conversion works through `?` and direct conversion,
retaining typed source links, a custom payload, and the exact caller location.
Names and the insertion anchor come from shared IR; collisions, unsupported
source types, stale snapshots, and corrupt facts refuse generation. The planner
only appends declarations; signature and caller migration remain independent
operations before composition into a complete apply unit.

`tests/strategy_error_return.rs` lifts an infallible function to the declaration
planner's selected error type. The contract is bound to the same module and
source snapshot; callers cannot supply an arbitrary type string. The test
combines the two previews in its disposable specimen, compiles without warnings,
and executes early and tail success paths. The original failure still panics:
this isolates success lifting from the subsequent propagation operation. Stale
contracts, different modules, existing Result contracts, and fixed signatures
are refused before returning edits.

`tests/strategy_error_caller.rs` checks an isolated caller relationship. The
planner retains the callee signature lift as a dependency, lifts the caller,
and adapts all its resolved calls to that callee. Stable edit ordering puts `?`
inside `Ok` for an exact-tail call. Executable comparisons check all branches
and evaluation counts; corrupted later call sites must refuse the entire
operation. This is one edge of a potentially transitive migration: changing the
caller creates obligations for its own callers, including function-value uses.

`tests/strategy_error_component.rs` exercises the production composed preview.
One shared snapshot supplies a fresh I/O error architecture, leaf unwrap
conversion, seven signature lifts, and every located call between the changed
functions. The fixed-point walk terminates on recursive cycles and deduplicates
diamond relationships. Success values and evaluation counts remain unchanged.
Failure returns the typed error through every caller and skips the remaining
successful-path calls. Compilation, deterministic preview, stale-byte refusal,
transitive boundary/call refusal, comments, and no-op recollection are checked.
The explicitly selected `entry` becomes Result and `main` observes it through
Debug. This proves a transitive preview, not complete use-site closure or a CLI
write: function values and unknown uses still need shared evidence.

`tests/strategy_function_uses.rs` checks the shared loader's written-use
inventory. Function values in constants and locals, qualified uses, impl and
inline-module bodies, nested functions, closures, async and const blocks all
survive persisted IR reload. Path and call ranges retain their original bytes;
macro tokens remain opaque obligations. Missing inventory on an older snapshot
means unknown, while malformed inventory returns an error. No downstream parse
or source scan supplies these facts.

`tests/strategy_closed_panic.rs` joins written uses to resolved call identities
before issuing a closed migration token. The eight-function executable keeps
successful output and evaluation counts, while failure returns its original
typed I/O source through `main` with Rust's error diagnostic and failure exit
status. An isolated boundary-adapter test leaves the leaf panic unchanged.
Cargo entry paths come from the existing metadata load into the shared IR;
a same-named non-entry function cannot qualify. Function values, unresolved
qualified or nested uses, opaque syntax, other source files, and missing or
inconsistent inventories prevent closure. Preview still never writes files.

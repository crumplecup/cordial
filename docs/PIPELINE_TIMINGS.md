# Pipeline progress and timings

`cordial submit --tier quick --deny-open --json` reports progress on stderr. A
counted bar advances after each loader, enricher, cache write, probe, assessor,
exception pass, reporter, and artifact write. Its message names the operation
currently running, so a long Cargo or rustdoc step remains identifiable while
the heartbeat runs. The number counts completed operations, not elapsed time.

Every completed, non-measurement run stores its latest timing breakdown in
`~/.cordial/<project>/ledger.json` under `last_run_timings` and
`last_run_wall_nanos`. Each entry has `stage`, `id`, `elapsed_nanos`, and
`invocations`. `phase` entries are inclusive major stages. `pipeline` entries
measure work between etiquette hooks, such as IR construction, cache writes,
exception handling, coverage summary generation, and report writes. Hook
stages (`load`, `enrich`, `probe`, `assess`, `workspace_assess`, `report`) retain
their existing per-hook measurements. Inclusive phases overlap their detailed
entries; do not add them together. Incomplete runs do not update the ledger.

`cordial timings` writes isolated cold and warm passes to `timings.json` and a
phase table to `timings.md` in the same store. Use these when comparing one
etiquette in isolation; use the ledger when inspecting a real multi-etiquette
gate. A cold and warm pair can have different Cargo cache state, so compare
like with like.

For deeper profiling, build with `cargo build --features hotpath`. The binary
writes a hotpath JSON report to a temporary file. Set `HOTPATH_OUTPUT_PATH` to
choose a stable report path, or set `RUST_LOG=info` to see its path on stderr.
The report stays separate from Cordial's JSON stdout. The hotpath feature is
opt-in; ordinary builds keep its instrumentation disabled.

The feature-warning scan has separate hotpath markers for checking whether
`cargo hack` is available, collecting private features, waiting for the
`cargo hack` subprocess, parsing its output, and building finding records.
The subprocess marker includes process startup, waiting, and captured output;
it does not profile the child process's own functions.

On 2026-10-18, a focused `feature_warnings` run with a warm Cargo cache took
240.26 seconds. The subprocess boundary took 231.46 seconds (96.34% of the
run); parsing took 0.33 seconds. The separate cold `all` tier run took 917.97
seconds, of which the feature-warning enricher took 879.04 seconds. These
absolute durations reflect different cache states, so use the focused profile
for the within-run split.

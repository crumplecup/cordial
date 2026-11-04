# `creusot_diagnostics` invokes a `cargo creusot` subcommand that no longer exists

**Status: resolved.** Both runner paths now omit `prove`. Regression tests check
the exact default and custom runner arguments, and fallback findings identify
invocation failures using error output instead of the first build-progress line.
The original report follows.

## Summary

`creusot-diagnostics-etiquette.md`'s documented runner contract invokes:

```sh
cargo creusot prove -- -p <crate>
```

As of Creusot 0.12, `cargo creusot` dropped the `prove` subcommand -- a bare
`cargo creusot -- -p <crate>` now runs both the translate and prove stages
in one invocation. Against the currently-installed Creusot CLI, the
documented invocation fails outright rather than running the proof:

```
Warning: found 'prove' pattern; it is no longer a 'cargo creusot' command. Just run 'cargo creusot'.
Error: No files to prove
```

## Where this was found

Working through `amenable`'s cordial worklist (`cordial open`/`done`),
`CREUSOT-DIAGNOSTIC-002` kept reporting a crate-level finding for
`amenable_creusot` whose `item_line` was just the literal text
`Compiling amenable_creusot v0.1.0 (...)` -- not a real diagnostic.

Per the etiquette doc's own description of the fallback path ("A nonzero
exit with no parseable `error:` span produces one crate-level
`CREUSOT-DIAGNOSTIC-002` finding at `Cargo.toml:1`"), this is exactly that
fallback firing: the `prove` invocation fails before producing any real
Creusot diagnostic output, so cordial has nothing to parse and falls back
to a crate-level finding built from whatever stray text it captured.

Confirmed by reproducing cordial's exact invocation pattern by hand:

```sh
env PATH="$HOME/.local/share/creusot/bin:${PATH}" \
    DUNE_DIR_LOCATIONS="why3find:lib:$HOME/.local/share/creusot/share/why3find" \
    WHY3CONFIG="$HOME/.config/creusot/why3.conf" \
    cargo creusot prove -- -p amenable_creusot
```

which produced the `Warning: ... it is no longer a 'cargo creusot' command`
/ `Error: No files to prove` output above. The real proof obligation is
unaffected and genuinely passes -- running the actual current invocation
(`cargo creusot -- -p amenable_creusot`, no `prove` subcommand, which is
what this repo's own `just verify-creusot` already uses) succeeds cleanly
("Proved (172 files) ✔"). So the finding is pure tooling-version drift in
`creusot_diagnostics`'s own runner, not a real crate defect.

## Why this is wrong

The etiquette's whole goal is "warnings and verifier failures become
normal Cordial findings" that read as genuine Creusot diagnostics. A
finding generated because the runner itself used a removed subcommand
doesn't reflect anything about the crate under test -- it's
indistinguishable, in the findings list, from an actual Creusot warning,
so it either gets investigated as if it were real crate debt (wasted
effort) or gets dismissed/excepted as noise (which then risks masking a
genuinely new real finding the next time the fallback fires for an
unrelated reason).

## Suggested direction (not investigated against the real implementation -- just the shape of the gap)

Drop the `prove` subcommand from the runner invocation --
`cargo creusot -- -p <crate>` (and the same for the `CORDIAL_CREUSOT`
custom-runner contract, which passes the same `prove -- -p <crate>`
arguments through). Current Creusot CLI versions treat a bare
`cargo creusot` invocation as doing both translate and prove, so this
should be a drop-in replacement rather than a behavior change needing a
new flag.

## Repro

```
cd amenable
cordial exceptions load
cordial submit --tier deep
# or: cordial open, then peek the amenable_creusot CREUSOT-DIAGNOSTIC-002 item
```

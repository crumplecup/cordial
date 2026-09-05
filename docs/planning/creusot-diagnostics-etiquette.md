# `creusot_diagnostics` etiquette

## Goal

Make `cargo creusot prove` part of the same feedback loop as source lints:
warnings and verifier failures become normal Cordial findings, checklist items,
CSV rows, and quality-report totals.

## Rules

| Rule | Meaning |
| --- | --- |
| `CREUSOT-DIAGNOSTIC-001` | A `warning:` diagnostic emitted by `cargo creusot prove` |
| `CREUSOT-DIAGNOSTIC-002` | An `error:` diagnostic, or a failed prove run without a parseable span |

## Target detection

The etiquette only invokes Creusot for crates that look like proof crates:

- the package directory name ends in `_creusot`
- `Cargo.toml` names `creusot-std`, `creusot_contracts`, or
  `creusot_contracts_proc` under dependency sections

Crates listed in `[creusot_diagnostics] skip_crates` are skipped before any
runner is invoked.

## Runner contract

By default the scanner looks for `cargo-creusot` on `PATH` and invokes:

```sh
cargo creusot prove -- -p <crate>
```

Tests and custom integrations can set `CORDIAL_CREUSOT` to an executable. The
custom runner receives:

```sh
<runner> prove -- -p <crate>
```

Stdout and stderr are parsed together. Rustc-style summary lines such as
`warning: 3 warnings emitted` are ignored. A nonzero exit with no parseable
`error:` span produces one crate-level `CREUSOT-DIAGNOSTIC-002` finding at
`Cargo.toml:1`.

## Config

```toml
[creusot_diagnostics]
enabled = true
skip_crates = ["legacy_creusot"]
```

Feature `creusot_diagnostics`, included in `quality`.

## Status

Implemented with parser, target-detection, config, fake-runner session tests,
and report artifacts in `tests/creusot_diagnostics_etiquette.rs`.

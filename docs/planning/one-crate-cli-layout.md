# One crate, CLI in the library

Cordial is a single package: library + binary. First-party `cordial_*`
satellites (`_cli`, `_elicitation`, `_homecoming`, `_amenable`) are rolled
up. Downstream plugin crates and `examples/custom_plugins` stay separate.
Parity fixture workspaces are tests, not product crates.

Complements [error-handling-as-plugin.md](error-handling-as-plugin.md) and
[CORDIAL_PLAN.md](../../CORDIAL_PLAN.md).

---

## Layout

```
cordial/                 # lib + bin `cordial`
src/lib.rs
src/main.rs              # parse + act + miette only
src/cli.rs               # Cli, Commands, Cli::act
src/error.rs             # one parent: CordialError
examples/custom_plugins/ # downstream template
```

`clap` and `miette` are optional on the `cli` feature. The binary requires
it. Library modules reachable from `lib.rs` do not import miette.

---

## Errors

One parent. CLI failures are native sources on `CordialErrorKind` (`NotFound`,
`NoExceptions`, `NoCachedIr`, `Prefix`, …), same tree as `Io` and `SynParse`.
There is no `CliError` and no bin-only `BinaryError`. Coverage is a clap
subcommand only when a coverage feature is compiled in. `main` converts the
umbrella at the edge:

```rust
fn main() -> miette::Result<()> {
    Cli::parse().act().into_diagnostic()
}
```

## Dispatch

`Parser` and `Subcommand` types live in the library and each implement
`fn act(self, …) -> Result`. If a clap type contains another clap type,
its `act` calls `act` on **each** nested clap value — `Cli::act` hands
off to `Commands::act`, which hands off to `ExceptionCommands::act` and
`ExportCommands::act`, and so on. Free functions do not take clap types
(including behind `Option` / `Box` / `Vec`). `main` only parses, calls
`act`, and converts with miette.

## Surfaces

| Crate shape | Policy |
| --- | --- |
| lib only | Existing error architecture. No CLI rules. |
| lib + bin, no clap | Thin bin: call the library, miette at the edge. No extra `Error` in bin-only files. |
| lib + bin + clap | CLI is not an island (rules below). |
| bin only | miette; clap in `main` is allowed (there is no library). |

Do not lint other people’s `_cli` crate names. That is cordial’s layout.

## Rules

| Rule | When |
| --- | --- |
| `CLI-ISLAND-001` | `Parser` / `Subcommand` types, or `Error` types, exist only on the binary side (`main.rs`, `src/bin`, modules only `main` includes) |
| `CLI-ACT-001` | Clap type in the library has no inherent `act(self) -> Result`; `act` does not call `act` on each nested clap type; or a free function takes a clap type (including behind `Option` / `Box`) |
| `CLI-MAIN-001` | `main` is more than parse + `act` + miette, or it does not call `parse` and `act` |

Implementation: `src/etiquettes/cli_layout/` (feature `cli_layout`, in `quality`).
Not part of `internal_error_chain`.

---

| Task | Detail |
| --- | --- |
| One-crate rollup (drop first-party `cordial_*`) | done |
| CLI errors on `CordialErrorKind`; delete `BinaryError` | done |
| `Cli::act` + thin `main` | done |
| CLI layout lints | done |
| Per-nested `act` handoff; peel `Option`/`Box` on clap args | done |
| Extract `cli_layout` etiquette; drop `CoverageFeatureDisabled` | done |

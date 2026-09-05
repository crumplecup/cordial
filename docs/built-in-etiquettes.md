# Built-in etiquettes

`cordial` ships standards, not isolated lints. Each etiquette is a named bundle
with an explanation, rule ids, optional quality-rollup row, and one or more
reporters. This tour groups them by the coding standard they enforce.

Use this page when deciding what a finding means. Use `cordial explain` when
you need the exact compiled explanation for the current binary:

```sh
cordial explain
cordial explain tracing
cordial explain TRACING-MISSING-INSTRUMENT
```

Rule ids are stable handles for reports, exceptions, and targeted explanations.
Etiquette ids are the names used in `cordial.toml` opt-outs.

## Error handling

The error-handling standard is: library failures return typed crate errors,
foreign causes stay inspectable through `source()`, and binaries/tests surface
errors at the boundary instead of aborting or stringifying them.

| Etiquette | Enforces | Main rule ids |
| --- | --- | --- |
| `panics` | Abort sites are explicit review items: `panic!`, `unreachable!`, `expect`, `unwrap`, and `compile_error!`. | `PANIC-SOURCE-*` |
| `error_sites` | Error propagation sites are inventoried before judging chain behavior. | `ERROR-SITE-QUESTION-MARK`, `ERROR-SITE-MAP-ERR`, `ERROR-SITE-RETURN-ERR`, `ERROR-SITE-IF-LET-ERR`, `ERROR-SITE-MATCH-ERR`, `ERROR-SITE-OK-OR` |
| `error_chain` | Converters and wrappers keep the original cause instead of dropping `source()`. | `ERROR-CHAIN-WRAPPER-SOURCE-001`, `ERROR-CHAIN-KIND-WRAPPER-PAYLOAD-001`, `ERROR-CHAIN-FROM-BRIDGE-001`, preserved-chain contrast rules |
| `internal_error_chain` | Internal error types follow the parent / `Kind` / native-source architecture. | `ERROR-CHAIN-COMPLIANCE-*`, `ERROR-CHAIN-INTERNAL-*` |
| `foreign_error_types` | Foreign `E` types leaking onto this crate's `Result` surface are made visible. | `FOREIGN-ERROR-CANDIDATE` |
| `foreign_error_attenuation` | Foreign error sites are classified as preserved, broken, pending infrastructure, or neutral. | `ERROR-HANDLING-CHAIN-PRESERVED`, `ERROR-HANDLING-CHAIN-BREAK`, `ERROR-HANDLING-PENDING-INFRA`, `ERROR-HANDLING-NEUTRAL` |
| `antipatterns` | Untyped carriers are treated as error-design findings, even though this etiquette also owns other smells. | `ANTIPATTERN-BOX-DYN-ERROR-001`, `ANTIPATTERN-STRING-ERROR-001` |

Read these together. `error_sites` says where failures move, `error_chain`
says whether the cause survived that movement, `foreign_error_*` explains the
foreign/local boundary, and `internal_error_chain` checks the crate-owned error
architecture. `panics` catches places that bypass the typed path entirely.
`antipatterns` also canaries strategy selection for `&'static str` fields:
`[antipatterns.static_refs] strategy = "string" | "cow" | "const"` changes the
recommended remediation while the rule keeps making runtime static borrows
visible. The `const` strategy is contextual: const/static-only types are quiet;
runtime or unclear construction falls back to `Cow<'static, str>`.

## Observability

The observability standard is: important work has spans, runtime boundaries
initialize tracing deliberately, leftover stdio debugging is removed,
compiler-signal suppressions are reviewable, and docs compile with the same
seriousness as code.

| Etiquette | Enforces | Main rule ids |
| --- | --- | --- |
| `tracing` | Functions follow the tracing recipe for their role, proof-only/skip-policy code is attenuated, subscribers are initialized deliberately, fallible boundaries report errors, and stdio macros are removed. | `TRACING-MISSING-INSTRUMENT`, `TRACING-LEVEL-MISMATCH`, `TRACING-SKIP-MISSING`, `TRACING-ERR-MISSING`, `TRACING-ERROR-PATH-SILENT`, `TRACING-FIELDS-MISSING`, `TRACING-PROOF-INSTRUMENT`, `TRACING-UNGATED-INSTRUMENT`, `TRACING-SKIP-INSTRUMENT`, `TRACING-SUBSCRIBER-*`, `TRACING-BOUNDARY-MAIN-SILENT`, `TRACING-STD-*` |
| `allows` | `#[allow(...)]` and `#![allow(...)]` suppressions are inventoried; Verus prelude allows need `reason = "..."`. | `ALLOW-ATTR-001`, `ALLOW-VERUS-REASON-001` |
| `doc_warnings` | `cargo doc` diagnostics are captured even when rustc and Clippy do not see them. | `DOC-WARNING-001` |
| `crate_attrs` | Library roots state the unsafe-code and missing-docs policy explicitly. | `CRATE-FORBID-UNSAFE-001`, `CRATE-MISSING-DOCS-001` |

The tracing etiquette is intentionally stricter than a public-API-only census:
private helpers can still be the missing span in an incident. Volume belongs in
subscriber filtering, not in selective omission.

## API shape

The API-shape standard is: public paths should earn their existence, contracts
should be easy to find, imports should name what they use, and boilerplate
should be delegated to derives where the crate policy allows it.

| Etiquette | Enforces | Main rule ids |
| --- | --- | --- |
| `visibility` | Small crates stay flat, visible modules have enough leaf names, and child visibility does not exceed parent visibility. | `VIS-CRATE-FLAT-001`, `VIS-MOD-THIN-001`, `VIS-MOD-MISMATCH-001` |
| `derives` | Hand-rolled builders, accessors, simple constructors, and public fields are reviewed as derive candidates. | `DERIVE-BUILDER-001`, `DERIVE-USE-BUILDER-001`, `DERIVE-GETTER-001`, `DERIVE-SETTER-001`, `DERIVE-ASREF-001`, `DERIVE-ASSTR-001`, `DERIVE-NEW-001`, `DERIVE-PUB-FIELD-001` |
| `pageantry` | Traits live in a leading block below imports and module declarations. | `PAGEANTRY-TRAIT-001` |
| `glob_imports` | Glob imports are replaced with explicit names. | `GLOB-IMPORT-001` |

`visibility` and `modularity` are intentionally separate. Visibility asks
whether an exposed module path is justified. Modularity asks whether the mass of
the code should be split, moved, or collapsed.

## Structure

The structure standard is: the codebase remains navigable. Large files and
functions are split, tests live in integration-test space, and CLI parsing is a
thin binary boundary over library behavior.

| Etiquette | Enforces | Main rule ids |
| --- | --- | --- |
| `modularity` | Oversized files/functions, overpacked files, statistical module outliers, top-heavy parents, lopsided siblings, and unary child directories are split or collapsed. | `MODULARITY-FILE`, `MODULARITY-FUNCTION`, `MODULARITY-TYPES-PER-FILE`, `MODULARITY-MODULE-SIZE`, `MODULARITY-TOP-HEAVY`, `MODULARITY-LOPSIDED`, `MODULARITY-COLLAPSE` |
| `inline_tests` | `#[cfg(test)]` and `#[test]` items move out of `src/` and into `tests/`. | `INLINE-TEST-MOD`, `INLINE-TEST-CFG`, `INLINE-TEST-FN` |
| `cli_layout` | Clap types live in the library, dispatch through `act`, and `main` stays parse + act + miette. | `CLI-ISLAND-001`, `CLI-ACT-001`, `CLI-MAIN-001` |

These lints make refactors cheaper. They do not say every module must be small;
they mark places where size or placement is now carrying architectural meaning.

## Conditional code

The conditional-code standard is: cfg names are declared, verifier-specific
gates stay in the right verifier crate, and repeated cfg logic moves toward
module boundaries.

| Etiquette | Enforces | Main rule ids |
| --- | --- | --- |
| `cfg_hygiene` | Undeclared cfg names and wrong-verifier cfg names are visible. | `UNEXPECTED-CFG-001`, `CFG-VERIFIER-MISMATCH-001` |
| `cfg_scatter` | Repeated item-level cfg predicates are replaced with a gated module when possible. | `CFG-SCATTER-001` |

`cfg_scatter` deliberately avoids field- and variant-only gating because those
often exist to hold a feature-gated type. It is looking for copied control flow
over free-standing items.

## Proof hygiene

The proof-hygiene standard is: verifier-only compiler signal and trusted proof
shortcuts are not invisible just because ordinary Rust tooling accepts them.

| Etiquette | Enforces | Main rule ids |
| --- | --- | --- |
| `verus_warnings` | Warnings from the Verus rustc fork are captured separately from ordinary rustc output. | `VERUS-WARNING-001` |
| `creusot_diagnostics` | `cargo creusot prove` warnings and verifier failures are captured when Creusot crates are present. | `CREUSOT-DIAGNOSTIC-001`, `CREUSOT-DIAGNOSTIC-002` |
| `proof_patterns` | Trusted Verus forms and implicit broadcast dependencies are inventoried. | `PROOF-PATTERN-ASSUME`, `PROOF-PATTERN-ADMIT`, `PROOF-PATTERN-EXTERNAL-BODY`, `PROOF-PATTERN-UNINTERP`, `PROOF-PATTERN-AXIOM`, `PROOF-PATTERN-BROADCAST` |
| `antipatterns` | Verifier contract bounds must be named where the supported shape requires it. | `ANTIPATTERN-UNNAMED-CONTRACT-BOUND-001` |

These are proof-surface checks, not proof obligations. They make trusted or
tool-specific behavior visible so reviewers can decide whether it is justified.

## Coverage

Coverage etiquettes answer inventory questions from rustdoc JSON. They do not
feed `quality-report.md` because a missing implementation is usually backlog or
surface-area accounting, not a source-quality violation by itself.

| Etiquette | Enforces | Main rule ids |
| --- | --- | --- |
| `impl-coverage` | Project types implement the required elicitation traits, including prerequisite traits. | `IMPL-COVERAGE-GAP` |
| `trenchcoat` | Foreign types are wrapped before they reach local elicitation traits. | `TRENCHCOAT-MISSING-WRAP` |
| `shadow` | Shadow crates mirror upstream items, including workspace-level gaps. | `SHADOW-MISSING-MIRROR` |
| `homecoming-std` | Rust `std` / `core` / `alloc` coverage for homecoming `Code` is inventoried. | `FRAMEWORK-STD-ROW` |
| `amenable-std` | Rust `std` coverage in the amenable registry is inventoried. | `AMENABLE-STD-ROW` |

Run `cordial build rustdoc` before coverage. Quality etiquettes can run from
source alone; coverage needs the rustdoc inventory.

## Keeping this current

This tour is intentionally higher-level than `cordial explain`. When behavior
changes, update in this order:

1. update the public trait or hook docs if the interface changed;
2. update the etiquette module docs and `EtiquetteExplain` metadata;
3. rerun `cordial explain` and refresh this tour;
4. update README links only if the reader path changed.

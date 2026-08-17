# Custom plugin example

User-facing extension story: a plugin is a **named family of related
etiquettes**, not a bag of hooks and not a kitchen-sink `CustomPlugin`.

Worked templates live in [`examples/custom_plugins`](../../examples/custom_plugins).
That crate depends on `cordial` from outside the first-party feature matrix.

---

## The three kinds

```text
SessionBuilder.register_plugin
        │
        ├── StaticPlugin          (Plugin only — quality family)
        ├── AcmeApiCoverage       (Coverage: Plugin)
        └── AcmeErrorHandling     (ErrorHandling: Plugin)
              │
              └── each returns &[&dyn Etiquette]
```

| Kind | Example | What to copy |
| --- | --- | --- |
| Plugin only | `ACME_STYLE` | [`StaticPlugin`](../../src/plugin/mod.rs) + one or more [`StaticEtiquette`](../../src/etiquette.rs) bundles |
| Coverage | `AcmeApiCoverage` | `impl Plugin` + `impl Coverage` (targets, trait requirement) |
| Error handling | `AcmeErrorHandling` | `impl Plugin` + `impl ErrorHandling` (scope, policy) |

Register:

```rust
let session = SessionBuilder::new(root)
    .register_plugin(&ACME_STYLE)
    .register_plugin(&ACME_API_COVERAGE)
    .register_plugin(&ACME_ERROR_HANDLING)
    .build();
```

---

## What not to do

- **`CustomPlugin`** — one type that can be “any kind.” The next family
  becomes a field on that type. Coverage already forbids this
  (`Do not add Option<EvidenceSource> to Coverage`).
- **Builders on `Coverage` / `ErrorHandling`** — those supertrait methods
  *are* the API. A builder that fills in `targets` or `layers` hides the
  impl a third-party crate is supposed to copy.
- **One etiquette as the whole story** — `tests/source_etiquette.rs` already
  shows a custom `StaticEtiquette`. The missing piece was the *plugin*
  layer: a family of related etiquettes under one id.

`EtiquettePlugin` stays for the 1:1 wrap used by built-in quality scanners.
`StaticPlugin` is the N-etiquette constructor for Plugin-only families.

---

## Example crate

[`examples/custom_plugins`](../../examples/custom_plugins):

- **`AcmeStyle`** — leftover `todo!()` sites; `StaticPlugin` at id
  `acme-style`.
- **`AcmeApiCoverage`** — `WorkspaceMembersTargetProvider` + a one-trait
  `Display` requirement; `etiquettes()` reuses `IMPL_COVERAGE_ETIQUETTE`.
- **`AcmeErrorHandling`** — default workspace scope, policy of sites+chain
  only; `etiquettes()` reuses `ERROR_SITES_ETIQUETTE` and
  `ERROR_CHAIN_ETIQUETTE`.

`tests/register.rs` asserts category routing for all three and runs the
source-scan families against a planted `todo!()`. The coverage plugin is
not executed in that fixture — `IMPL_COVERAGE_ETIQUETTE` needs rustdoc JSON.

Out of scope: CLI flags for third-party plugins; relaxing `'static` on
`SessionBuilder::register_plugin`.

## Status

Implemented. `StaticPlugin` is in core; the example crate compiles and its
registration test passes.

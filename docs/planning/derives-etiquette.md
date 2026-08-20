# Derive patterns etiquette

Flags hand-rolled constructors and accessors that a derive crate (or a
builder) should write. Policy, not a compile error. Complements tracing
(role instrumentation) and visibility (module paths). Pub *fields* stay
here (`DERIVE-PUB-FIELD-001`); pub *modules* are visibility.

Knobs live in `cordial.toml` (`[derives]`), loaded through
[`load_session_config`](../../src/config.rs). See
[cordial-config.md](cordial-config.md).

---

## Rules

| Rule | Meaning |
| --- | --- |
| `DERIVE-BUILDER-001` | Hand-rolled `*Builder` type, `build(self)`, or enough fluent `mut self` setters — use `#[derive(derive_builder::Builder)]` |
| `DERIVE-USE-BUILDER-001` | `fn new` has more arguments than `max_constructor_args` — introduce a builder |
| `DERIVE-NEW-001` | Trivial `fn new` at or below that arity — `#[derive(derive_new::new)]` if there is no validation |
| `DERIVE-GETTER-001` | `fn foo(&self)` returns private field `foo` (`&self.foo` / `self.foo` / `.clone()`). `.clone()` / Copy → `#[getter(copy)]`. |
| `DERIVE-SETTER-001` | `with_*` / `set_*` that assigns the argument (optionally `.into()` / `Some(arg)`). `#[setters(into)]` and `#[setters(strip_option)]` as needed. |
| `DERIVE-ASREF-001` | `self.field.as_ref()` → `#[derive(derive_more::AsRef)]` |
| `DERIVE-ASSTR-001` | `self.field.as_str()` → `#[derive(derive_more::AsRef)]` with `#[as_ref]` for `AsRef<str>` |
| `DERIVE-PUB-FIELD-001` | Any non-inherited field visibility |

Error types (`impl Error for …`) and `#[track_caller]` constructors skip
`DERIVE-NEW-001` and `DERIVE-USE-BUILDER-001`. Native sources need
`Location::caller()`; `derive_new` cannot emit `#[track_caller]`.

Clap schema types (`#[derive(Parser)]`, `Args`, `Subcommand`) skip
`DERIVE-PUB-FIELD-001`. The struct is the CLI schema (field = argument);
`cli_layout` already catalogs those derives. `pub(super)` / `pub(crate)`
bags still flag — non-inherited is the rule, not “escapes the crate.”

Getters and setters fire when the body is a derive option: a field read
(or `.clone()`), a field assign, `arg.into()`, or `Some(arg)`. Extra
statements or extra parameters still skip. Fluent `mut self` methods
count toward `min_fluent_setters` when they are those setters.
`as_ref()` / `as_str()` are `derive_more::AsRef` (`AsRef<str>` for
`as_str`; derive_more has no separate `AsStr` macro).

## Thresholds

```toml
[derives]
max_constructor_args = 3
min_fluent_setters = 2
```

| Knob | Default | Effect |
| --- | ---: | --- |
| `max_constructor_args` | 3 | `new` with more args must use a builder; at or below, trivial `new` may be `derive_new` |
| `min_fluent_setters` | 2 | Trivial inherent `mut self` setters at this count mean a hand-rolled builder |

## Status

Implemented (`src/etiquettes/derives/`, feature `derives`, in `quality`).
Artifacts: `derives.checklist.md`, `derives-summary.md`, `derives.csv`.

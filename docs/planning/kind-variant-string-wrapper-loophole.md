# Stringified error source chain gap

Status: **Done**

## Reported symptom

`amenable_time::error::InvalidDescriptorSource` and
`ParseRejectedSource` can receive `format!("{err}")` or `err.to_string()`
from a caught error. The new string becomes the native source's message,
and the original error is no longer available through `Error::source`.

For example, `builder.build().map_err(|err| InvalidDescriptorSource::new(
format!("could not build: {err}")))` was not flagged. Cordial inferred
foreign error types from a small set of known receiver expressions before
running `StringifyForeign001`; it cannot infer the type of `builder.build()`
from syntax alone. That early return was the immediate escape hatch.

## Separate fanout policy

The maintainer requested `kind_fanout_max_call_sites` to default to zero.
This is a valid frequency policy in its own right. It flags matching literal
call sites regardless of how the error payload is represented, so it cannot
enforce preservation of a typed source chain. The default is now zero.

## Fix

Inspect the error binding at conversion sites before the receiver-type
inference guard. A `map_err` closure, `if let Err(...)`, or `match` error arm
that converts its bound error with `format!` or `.to_string()` and does not
pass that binding into the resulting error gets `StringifyForeign001`.
Known typed-error constructors retain the existing `DiscardTyped001` finding.
The rule id stays stable. Regression fixtures cover an uninferable receiver,
the three conversion forms, and a wrapper that retains the typed source.

The original proposal was to reject every native-source struct whose
content fields are all `String`. That shape alone does not establish that
an earlier error was stringified: a fresh invariant error may legitimately
carry a message. The conversion site supplies the evidence for this rule.

## Limits

This is a syntax-based lint. It recognizes direct binding use in common
conversion forms; an arbitrary helper can still hide stringification or
source retention. A broader guarantee requires typed data-flow analysis or
an API that requires a typed source when wrapping an existing error.

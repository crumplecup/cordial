# Shared source provenance

**Status:** In progress. Implement in small slices; run focused checks and a
zero-open Cordial gate before each phase commit.

## Invariant

Every outer Rust file analyzed by a production source hook must use the parsed
tree issued by that session's shared source view. A scanner must not be able to
submit an independently parsed `syn::File` as if it came from that view. The
source text, path, syntax tree, and provenance travel together in one type with
private fields and no public constructor. A separate token passed beside raw
data would permit the data and token to be mismatched.

The cache now issues a `SharedParsedFile` that keeps those four facts together,
and a `SourceEnricher` receives a typed source view. This is an incremental
boundary: most production enrichers still implement the older `IrEnricher`
contract, and many scanners still accept `&str` and `&Path` and call
`syn::parse_file` themselves. See [`SourceLoadView`](../../src/loader/source.rs),
[`SourceEnricher`](../../src/hooks/enricher.rs), and the
[`modularity` scanner](../../src/etiquettes/modularity/scan.rs) for the first
migrated route. The glob-import and inline-test production scanners now use
the same source view, including files reached under `tests/`.

## API shape

- The source view issues an opaque `SharedParsedFile` (name provisional) for a
  requested path. It owns or retains the exact source text and `syn::File` from
  one cache entry, plus the path and a session provenance brand. Its fields and
  constructors stay private to the loader. The view alone creates it.
- Production source scanner entry points take `&SharedParsedFile`, or a narrower
  view derived from it, rather than separate source, path, and syntax arguments.
  They obtain the source, path, and syntax through accessors on that same value.
  The source hook interface should make this value the normal input so a plugin
  author does not have to downcast `LoadView` and choose a parser.
- Facts that claim a source location should be derived from this file view.
  If facts must cross into the dynamic graph, provide a checked conversion at
  that boundary; avoid accepting an unrelated path or span alongside the file
  brand. A per-file brand or a scoped callback may be needed to prevent a node
  from one file being paired with another file's path. Choose this after
  prototyping the smallest useful scanner interface.
- Keep the shared parse lazy. The source loader already snapshots `src/`; scans
  may also visit `tests/` and other roots. A first request reads and parses a
  path, and later requests in that session reuse the same entry. Keep the
  current thread-local ownership constraint of `syn::File` in view when shaping
  the hook API.
- Parsing embedded Rust strings or macro-specific token grammars is a separate
  operation. It must have an explicit source category rather than masquerade
  as an outer file obtained from the shared view.

## Migration

1. Introduce the sealed parsed-file carrier and change the shared cache to
   return it. Prove that callers cannot construct one from a raw `syn::File` or
   pair it with an unrelated source/path. Keep the existing cached call sites
   working through its accessors while defining the source-hook entry point.
2. Migrate production source scanners by family. At each entry point, replace
   raw source/path/syntax parameters with the carrier, then remove independent
   outer-file reads and parses from that path. Retain dedicated standalone
   helpers for tests or external use only if they cannot feed the production
   scanner with unbranded syntax. Preserve each scanner's file selection,
   fixture exclusions, and finding semantics.
3. Move source-derived location construction behind the carrier where needed.
   Audit graph attributes and finding conversion paths for raw paths or spans
   that can be paired with another file's syntax. Use typed, fallible
   conversions for required facts instead of defaults that hide a broken
   provenance link. This overlaps the typed-facts work in the
   [etiquette API review](etiquette-api-review.md).
4. Remove production APIs that accept plain `syn::File` or source text for an
   outer file. Update the custom-plugin example and writing-etiquettes guidance
   to teach the enforced source-hook contract, then narrow or restore their
   "parsed once" claim to the measured behavior.

## Acceptance checks

- Compile-fail coverage shows that a plugin cannot construct the carrier, use
  an independently parsed tree at a production scanner boundary, or swap a
  file's provenance with another file's source data.
- Instrumented combined runs report at most one shared outer-file parse per
  `(session, path)`, including paths reached by multiple etiquettes. Compare
  per-file parse counts and elapsed time with the existing source-work and
  hotpath output; do not infer this from a cache hit count alone.
- Focused scanner tests and a before/after finding comparison show that file
  selection and finding payloads have not drifted. Complete the relevant
  Cordial tier with zero open items before each phase commit.
- A source audit finds no direct outer-file `syn::parse_file` use in production
  scanners. Any remaining direct parse has an explicit embedded-source or
  standalone-test purpose.

## First slice decision

Modularity prototypes a `SourceEnricher` companion trait. Its adapter fixes the
required loader to `SourceLoader` and passes a typed `SourceEnrichView`, while
the scanner accepts `SharedParsedFile`. This removes the downcast and direct
file parse from that production route. The older `IrEnricher` contract remains
for migration; the invariant is not repo-wide until those source hooks move.

## Second slice

Glob-import and inline-test enrichers now use `SourceEnricher` and obtain each
outer file through `SourceLoadView::parsed_file`. Their public standalone scan
helpers remain for direct use and fixtures; production entry points accept the
provenance-bearing file. The next migration should move the remaining source
families onto this route and then retire standalone helpers from production
surfaces.

## Third slice

The source view now walks a caller-selected Rust file tree and returns parsed
files from its cache. Glob-import and inline-test scans use that walk, and the
cfg-scatter and derive production scans now use the typed hook and shared
files. Their standalone scan helpers remain available for direct callers.

## Fourth slice

Pageantry and cfg-hygiene production hooks now use the typed source view and
shared parsed files. The source view preserves pageantry's crate scan roots and
fixture exclusions, while cfg hygiene still scans only `src/`. Their public
standalone scanners remain available for direct callers and tests.

## Fifth slice

The antipattern production hook now takes a typed source view. Its first pass
collects crate-wide trait and construction facts from shared parsed files, and
its second pass visits those same trees. The standalone crate scan remains for
direct callers. Contract-bound scans use separate macro-specific grammars and
need their own audit before changing their parse path.

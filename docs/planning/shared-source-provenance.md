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
and a `SourceEnricher` receives a typed source view. Production source
enrichers have been migrated across the quality, error, tracing, and Verus
families. Standalone APIs and apply still need the same source provenance and
IR evidence contract; their remaining exceptions are tracked below. See
[`SourceLoadView`](../../src/loader/source.rs),
[`SourceEnricher`](../../src/hooks/enricher.rs), and the
[`modularity` scanner](../../src/etiquettes/modularity/scan.rs) for this route.

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
- Apply consumes source-derived decisions and edit locations from IR and never
  reparses a proposed file. Standalone production entry points use the shared
  loader and materialize their evidence in IR as well.

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

## Sixth slice

The crate-attribute inventory now inspects the library root through the shared
parsed file. It retains `[lib] path` handling and the existing missing-library
behavior. The tracing-print inventory now uses shared files across its crate
scan roots while retaining its configured file exclusions. Both standalone
scanners and the separate file-writing crate-attribute apply command retain
their direct parsing paths.

## Seventh slice

Tracing-subscriber and tracing-boundary production hooks now collect function
sites from shared parsed files. Both preserve their crate scan roots, fixture
exclusions, helper analysis, and standalone direct scan functions. The
baseline source-work report from the previously installed binary recorded 693
shared parses for 693 distinct files and 8,942 cache reuses on a build-tier
submit; direct parses are not included in those counts.

## Eighth slice

The tracing function inventory now obtains its outer files from the shared
source view, while its one-file helper remains available for fixture tests.
The panic inventory already used shared parsed files in production; its hook
now uses the typed `SourceEnricher` interface as well. The tracing call-graph
collector still parses workspace files separately. Its workspace-wide scope
needs a cache lifetime that spans individual crate source views before it can
share those parsed trees safely.

## Ninth slice

The visibility inventory now follows the crate's module tree through shared
parsed files, including out-of-line modules and `#[path]` modules. Its public
standalone scanner retains direct parsing for callers outside a source session.

## Tenth slice

The CLI-layout production scan now reuses each shared tree across module
discovery, clap/error type cataloging, and thin-main inspection. The public
standalone scan retains its direct parser for callers outside a source session.

## Eleventh slice

The unified error IR scan now enters through the typed source hook. The
internal-error architecture scan also reuses shared parsed files while
discovering out-of-line modules, in addition to its existing shared catalog
pass. Standalone crate scans retain their direct parser for callers outside a
source session.

## Twelfth slice

The attribute, allow, and foreign-error-attenuation enrichers now enter through
the typed source hook. The first two already used shared parsed files in their
production scans; attenuation joins facts already in the IR. This removes
their loader downcasts without changing their scan results.

## Thirteenth slice

The parsed-file cache now lives for the whole load-and-probe run, so later
crate views reuse trees opened by earlier workspace-wide scans. Path-inclusion
discovery and the tracing call graph obtain their production syntax through
that source view. Their standalone apply APIs retain direct parsing. The
workspace facts caches distinguished sourced runs by session identity at
this stage; a later phase removed those process-global caches.

## Fourteenth slice

The production Verus IR collector now discovers macro invocations in shared
outer-file trees. Panic and proof-pattern hooks reuse one Verus IR per crate
and source session, avoiding a second Verus macro parse as well. Standalone
Verus scans keep their direct parser; parsing the macro body's Verus grammar
remains a separate, explicit operation.

## Fifteenth slice

The Verus-warning production hook now uses the typed source view. When a
compiler warning needs the Verus pattern-projection filter, that filter
obtains the already-cached Verus IR from the same run. The direct standalone
warning scan remains available outside a source session.

## Sixteenth slice

The feature-warning, doc-warning, and Creusot-diagnostic enrichers now use the
typed source hook for their crate context. These hooks invoke external tools
and do not parse outer Rust files themselves. The public plugin example and
Part 2 of the writing-etiquettes series now request the shared parsed file
instead of reparsing source text. The introduction narrows its parsed-once
claim to outer Rust files in a source run.

## Seventeenth slice

The antipatterns contract-bounds pass was the last production outer-file parse
found by the source audit. Its Creusot, Verus, and Kani paths now consume a
`SharedParsedFile` through the source view. Standalone one-file scans retain
their direct parser. The separate parsing of verifier macro contents
remains necessary for those grammars. The production Kani session and the
standalone contract shape matrix pass after this change.

## Eighteenth slice

The apply audit found duplicate parses within individual operations. Crate
attribute apply now uses one syntax tree for the lint decision and insertion
position. Tracing apply uses one short-lived source view for path inclusion,
the call graph, and each file's recipe scan. Its view is dropped before the
subsequent quality run, so files changed by apply are parsed from their new
contents. The standalone antipattern crate scan also uses a short-lived source
view across its trait inventory and contract-bound pass. A one-file helper that
receives raw source still needs one parse when invoked alone; it cannot reuse
another invocation without a shared view.

## Source-work comparison

A build-tier run before the migration recorded 693 shared outer-file parses
for 693 distinct paths, 3,872 walks, and 8,942 cache reuses. The final
installed binary recorded 693 parses for 693 distinct paths, 3,872 walks, and
15,202 reuses. Neither run recorded a path parsed twice in its session. The
increase in reuse reflects the scanners now asking the shared view for trees
that they formerly parsed outside this counter. The baseline counter did not
instrument those direct parses, so these figures do not quantify the removed
work or prove an elapsed-time improvement. The focused scanner and apply tests
cover finding and patch behavior across the migrated paths.

The 6,260 additional cache reuses provide an illustrative scale for the work
removed. Multiplying each file's extra reuse count by its baseline shared-parse
cost gives 4.94 seconds of parse-equivalent work in this build-tier run, **if**
each extra reuse displaced one direct parse. The old direct parses were outside
the counter, so this is a scenario estimate, not measured time saved. The
measured shared-parse total changed from 0.566 to 0.587 seconds.

An isolated `tracing` cold/warm measurement on 2026-10-21 used the installed
binary after the source migration:

| Project | Cold (s) | Warm (s) | Largest warm detail |
| --- | ---: | ---: | --- |
| Cordial | 7.00 | 5.39 | IR cache write 2.59 s |
| Amenable | 7.56 | 2.77 | function inventory 0.98 s |
| Homecoming | 8.36 | 1.00 | setup 0.57 s |

The 2026-10-17 pre-migration measurements were Cordial 5.97/5.90 s and
Amenable 6.19/6.01 s. Cordial's warm pass is 0.51 s lower, but these are
single samples with different Cargo and filesystem cache states and intervening
code changes. They do not isolate the parsing refactor. Homecoming's cold setup
took 4.64 s, dropping to 0.57 s warm. Cordial's 2.59 s warm IR cache write is
a larger optimization candidate than the 0.59 s of shared parsing measured in
the combined build-tier run. In an earlier all-tier Cordial run,
feature-warning inventory occupied 662.0 of 667.1 wall seconds, so Cargo Hack
still dominates that gate.

## Current audit: apply and source-derived side channels

The earlier production-scanner audit was too narrow. Error-field apply parsed
the patched file a second time after planning edits. That parse is removed;
source identity, UTF-8 boundaries, and nonoverlapping edit ranges are checked
before writing, while apply fixtures compile representative output. Crate-attrs
apply now obtains its original tree through `SourceLoadView` and reads its lint
decision and insertion point from the crate IR. The historical descriptions
above record the earlier state, not an exception to the current invariant.

Crate-attrs' Verus exemption now checks source snapshots already loaded by
`SourceLoadView`; the exemption decision is stored on the crate IR root.
Standalone crate-attrs scans use the same loader path. The remaining
source-derived side channels are concrete follow-up work:

- Derive path inclusions and tracing call reachability now live once in
  `WorkspaceIr` for a run. The source enrichers read them through typed IR
  accessors; the process-global facts caches are gone. The public standalone
  path-inclusion API also obtains trees from a short-lived shared source view.
- Tracing apply still builds path-inclusion and call-graph facts in its own
  short-lived source view and classifies checklist rows directly. It must join
  the shared IR path for its decisions before the apply invariant is complete.
- Public standalone scan helpers still accept raw source or read files and may
  parse outer files directly. These cannot feed a production source hook today,
  but they remain an exception to a literal repo-wide one-parse policy. Move
  each standalone workflow onto the shared loader and IR before claiming the
  stronger invariant.

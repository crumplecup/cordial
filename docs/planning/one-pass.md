# One pass, one parse

Status: **Active**. Slices 1 to 3 are in the code; what is left is listed at the end.

## The problem

A run reads each file's syntax tree many times. The per-file collectors are separate Salsa queries
and each projection step is a sweep over every file asking for its collector's result, so a file is
asked about by about a dozen steps in sequence. While every tree stays in memory that costs nothing
but memory (see `ra-persistence.md`: a run keeps every tree it parses). As soon as the trees are
bounded (`SemanticCargoOptions::with_evicts_between_chunks`) each sweep re-parses what the last one
evicted, since a repeated sweep over more files than the capacity is the worst case for any
recency policy. Bounding memory without paying for it needs each file's collectors to run together,
while its tree is held, so that it is parsed once.

The target is one parse per file in the steps that read syntax, whatever the capacity.

## What exists

Two levels, already, in the code:

- **Extraction**: a per-file query that needs the tree. `scanned(db, (file, path), name, |source,
  sema| Collector::native_file(source, ..))` builds a `DatabaseSource` (path, text, tree, line
  index) and a `Semantics` for each call, and runs one collector. The collectors are pure functions
  of that carrier and of small semantic callbacks (`is_verus_macro`, `macro_by_path`, `macro_at`).
- **Aggregation**: queries that fold the per-file facts and need no tree: `panic_graph`,
  `antipattern_crate_facts`, `architecture_catalog`, `signature_index`, `apply_*`.

Per-file extractors (the steps that call them are in `projection/`):

| Extractor | Steps | Reads beyond the carrier |
| --- | --- | --- |
| `error_sites_file`, `error_chains_file`, `error_types_file`, `error_apply_file` | error_* | scan tree, root |
| `error_compliance_file` | error_compliance | scan tree, crate name, proc-macro flag |
| `observability_file`, `verus_file`, `visibility_file` | observability, verus, visibility | macro identity (semantics) |
| `policy_file` | ordinary projection | root, macro identity |
| `tracing_inventory_file` | declarations | semantics (definitions in the file) |
| `architecture_file`, `signature_file`, `path_inclusion_file` | architecture, signatures, path inclusion | scan tree / package |
| `panic_proofs_file` | panics | |
| `antipattern_facts_file`, `antipattern_contracts_file` | antipatterns | crate name |
| `file_facts`, `file_items` | evidence, items | semantics |

Two collectors need the tree **twice**, the second time under a crate-level fact:

| | First visit (tree) | Crate level | Second visit (tree) |
| --- | --- | --- | --- |
| panics | `panic_proofs_file` | `panic_graph` -> `panic_reachable` | `panic_scan_file(file, reachable)` |
| antipatterns | `antipattern_facts_file` | `antipattern_crate_facts` | `antipattern_sites_file(file, facts)` |

Steps that parse without a memoized query: the fallbacks `*_direct` (a file a query declined),
`error_signatures`, `error_architecture`, the leaf pass. (`cli_layout` was one; see "What was built".)

The steps do not agree on a key: they pass the file, path and root, and differ in the scan `tree`
(some pass an empty path) and, for compliance only, the crate name and a flag. A collector that
ignores a field is not changed by a fuller key.

## The design

1. **One extraction query per file and target**: `extract_file(db, key) -> Option<Extracted>`,
   keyed by file, path, scan tree, root, crate name and flag. It builds the carrier and one
   `Semantics` once, parses once, and runs every collector, returning a struct with one field per
   collector (the fields of features that are off are absent). `Extracted` is plain data and is
   persisted. The carrier is not a memo: it holds the tree.
2. **Per-collector projection queries** over it: `visibility_of(db, key) =
   extract_file(db, key).visibility.clone()`, and so on. Salsa backdates them: an edit that does
   not change a collector's field does not invalidate what read it. The aggregation queries and the
   steps read these, as they read the old per-file queries, so they are unchanged.
3. **Priming**: the pass is run file by file, in chunks, with an eviction and a wait for the dropped
   trees between chunks (the machinery of `eviction.rs`), so the trees held are a chunk's. Steps then
   find memos and never parse. On a restored run the memos are valid and the pass runs nothing.
4. **The two second visits become a filter over facts.** The first visit extracts every candidate
   with what is needed to decide it later (for panics: each panic site with its enclosing function;
   for antipatterns: each candidate with the facts it would consult), and the crate-level join and
   the filter run on facts, with no tree. This is the open design problem, and the one that
   decides whether the pass is truly one visit: it needs reading `file_scan` and `file_candidates`
   to see whether the crate fact is consulted per candidate (separable) or steers the walk (not).
5. **The direct sweeps** (`cli_layout`, `error_signatures`, `error_architecture`) become collectors
   of `Extracted`, or per-file queries run in the pass.

What this gives up: a field of `Extracted` is recomputed with the others when the file's text or any
of their semantic inputs changes. Before, a collector depended on its own inputs only. The
projection queries recover the downstream cut-off, not the recomputation.

## Acceptance

- A projection of a crate of more files than the parse capacity, with eviction on, executes
  `EditionedFileId::parse` about once per file (the `execution.*` metrics with
  `CORDIAL_QUERY_STEPS`), and holds trees of a chunk at the end of priming.
- The IR it produces equals the IR of today's, on the specimens and on elicit_doc.
- The edit battery of `tests/edit_reuse.rs` passes: an edit still reaches only what read it.

## What was built

1. **`extract_file`** runs the eight collectors that read only a file's tree (error sites, chains,
   types and apply; observability; verus; visibility; the policy part) on one carrier from one parse.
   Their queries are projections over it with the signatures they had, so the steps are unchanged.
2. **The priming pass** (`file_scans.rs`) asks, for each file, every first-visit query that applies to
   it, together on one worker so that they share the parse, in chunks with an eviction between them
   (`SemanticCargoOptions::with_evicts_between_chunks`). It asks the saved queries, not the extraction,
   so a restored run computes nothing. Compliance, the panic proofs and candidates, the antipattern
   facts, contracts and pending candidates, the architecture and signature parts and the module
   declarations are asked there too, and the tracing inventories from the step that has the outline.
3. **The second visits are gone.** Panics keep each site with the function it is in, final records
   and nested scans in order (`PanicCandidate`), and the reachable set of the crate's proofs is
   applied afterwards, de-duplication included, to the saved candidates. Antipatterns keep, for a site
   in a trait impl, the trait, and for a static field the traits its references name and the owner's
   name (`PendingCandidate`); the crate's traits, constants, runtime and error types are applied
   afterwards.
4. **Module discovery is a query.** What a `mod` declaration may lead to is read from the syntax
   (`ModuleTarget`, saved per file); whether a file exists is answered from the file set of the source
   root of the declaring file, which is an input of the database; `module_tree` follows the
   declarations from the entry file. Nothing in it reads the file system. A new `.rs` file the database
   never loaded is a reload in `sync_from_disk`, so the file set does not change within a session.
5. **The parse-error validation** of the panics step uses rust-analyzer's `parse_errors` firewall query.

6. **The CLI layout is a saved per-file query** (`layout_queries.rs`, `cli_layout_file`), asked by the
   priming for every file under `src`. What a file contributes was plain data except the bodies of its
   `act` functions, which were syntax nodes held until the crate's types were all known: they pinned
   every tree that has an `act` and made the result unsaveable. They are now data (`act_shape.rs`):
   what the dispatch resolution reads of an expression (path, field, a method that keeps the type),
   of a pattern (what it binds) and of the `match`, `if let`, `while let` and `let` that scope the
   bindings; `dispatch.rs` runs the same resolution over it. The files it follows and the
   existence checks of `lib.rs`, `main.rs` and `bin/` read the source root's file set (the
   `LayoutFiles` trait, answered by `module_queries`), not the file system.

7. **The files a crate has are a query** (`listing.rs`, `held_rust_files`), over the file set of the
   source root of the crate's entry file, and the collectors take that list (`HeldFiles`) instead of
   walking directories: error sites, chains, types, compliance, signatures and architecture,
   observability, verus, panics, antipatterns, the priming pass and the entry-less branch of
   `error_module_files`. A file the database does not hold is not scanned, which is what the
   collectors already needed (they ask the database for each file's id). The walks that remain read
   what to load, not what is loaded: `selection.rs` (`rust_files`), `populate.rs` and `sync.rs`.

The acceptance is a test (`tests/one_parse.rs`), with a ratchet that names any step that parses a file
for a collector (none, on the specimen) and a control that fails when the pass is skipped.

## What is left

- The sweeps that read the tree without a query: the fallbacks that run when a query declines a file
  (`*_direct`, `error_signatures`, `error_architecture`'s direct path). They run only for a file with
  a syntax error, to say why, and are not on the way of a clean crate.
- The leaf and context passes copy files per context and have their own sweeps.
- Not yet measured on elicit_doc or on cordial.

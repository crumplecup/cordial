# Save, sharing and context cost

Status: **Plan**, 2026-11-22. Four workstreams, in this order: **A** incremental save,
**B** Salsa flattening, **C** base layers (sysroot first), **D** cfg-context and leaf cost.
Nothing here is built. It follows [memory reduction](memory-reduction.md), whose measured
runs found that the peak is the save and that the remaining time is the save and the contexts.

## What the evidence says

Every number is from a cold amenable run (`--stop-after source_ir`, 122 s, peak 3.74 GiB) unless
another source is named. **Measured**, **read** (in the code) and **inferred** are kept apart; the
unknowns are listed at the end.

**The save.** It is 25 s of 122 (verification 8.5 s, encoding 16.6 s) and writes 593 MB every
run, a whole database each time. The peak is 1.07 GiB above the 2.67 GiB the analysis holds, and
it happens inside verification, where only 633 queries run. *Measured.*

**Where the 593 MB is.** The file has 91 ingredients, and five hold 77% of the bytes: 165 MB,
137 MB, 79 MB, 47 MB and 31 MB. The largest has 12,324 slots at about 13 KB each, which is the
file-text input (`FileText`): it is persisted with the whole text of every file as a string
(`serialize_file_text`, *read*), and most of those files are the sysroot (94 MB in 5,278 files) and
dependencies that do not change between runs. The next two are a 117K-memo query (1.2 KB a memo)
and a per-crate query with 698 memos at 113 KB each. A one-file project saves a 102 MB state, which
is the sysroot's share of every project's. *Measured; the ingredient names are by shape and memo
count, to be confirmed in A0.*

**How a memo is saved.** `Database { format, runtime, ingredients }` is one MessagePack stream:
structs first, then functions (*read*). Each function ingredient writes its `universe` (a list of
dependency edges), its `sets` (each a list of runs over the universe) and then its memos, which
refer to sets by number. A fresh `EdgeFlattener` is made per ingredient in `serialize` and again in
`verify_by_closure` (`function.rs`, *read*), and the parts of the verification run on one thread
each. The sets are held as dense bitsets (`Bits(Box<[u64]>)`, trimmed of trailing zero words) and
numbered by first appearance, so a set is as long as its highest edge number, though it is saved as
a few runs. *Read.*

**How much is redone.** Summed over the 44 ingredients that are flattened (786,733 memos), the walk
visits 2.17M non-persisted queries, against 556K for the largest ingredient alone: 3.9 times the
distinct work. One ingredient, `direct_facts`, has 63K memos, 137K distinct sets and 4.4M set edges.
*Measured.* That is a few hundred MB of dense sets alone if the universe is tens of thousands of
edges, which fits the transient. *Inferred, not yet measured: nothing logs the universe size or the
bytes the sets take.*

**What a rebuild costs.** The saved state is keyed by the executable's path, size and modification
time (`executable_fingerprint`), so every rebuild of cordial discards it and the next run is cold.
*Read.* The dependency layer's queries are persisted in part already (definition maps, impl indexes,
`lang_items`; see [rust-analyzer persistence](ra-persistence.md)).

**Contexts.** Six ordinary contexts take 13 s. The six leaf contexts take 34 s: 0.2 s entering them
and 30.6 s reading, on worker threads. A context covers about 1,015 files: 39% are shared with the
ordinary build, 9% flip a gate, 48% are "unsure" (leaf files, files that read the cfg some way the
gates do not say, files outside the ordinary module tree), 5% are the module spine. *Measured.*

**Not known.** The names of the largest ingredients; what is dirty after an edit; whether the
verified-at revision is saved in a way that makes every ingredient change on every save; how much of
the verification transient is the sets and how much is the parallel flatteners; whether a base
snapshot can be made deterministic when priming interns ids from several threads; how much of the
leaf cost is the first context and how much the later ones.

## Principles

- **Reproduce small, then fix.** Each step starts with a unit-test fixture that shows the problem
  (the verification fixtures and their ratchet, `tests/verify_before_save.rs`, are the model) and
  ends with that fixture green. No whole-program run decides a design.
- **Measure before building.** Each workstream starts with the events and counters it needs; they
  stay in the library.
- **Additive first.** New methods and a new format version beside the old; a state the new reader
  cannot use is a cold run, which is the fallback that exists today.
- **A run at a checkpoint, not after each change.** One amenable run (and elicit_doc) after the
  checkpoint named in each workstream, and the Cordial run only after the last.

## A. Incremental save

**Goal.** A save writes what changed. A warm run with a small edit writes a few MB and does not
flatten or encode the rest; a cold run is no slower than today.

**A0. Answered (2026-11-22), by reading the save code and comparing two real saves.** A small project
was run cold, a comment line was added to one file, and it was run again from the first state; the two
saved states were compared entry by entry.
1. *What changes.* 34,638 of 96,696 entries (36%) differ, across 60 of 88 ingredients, and **34,546 of
   those (99.7%) differ only in a revision stamp, `7` became `8`**. 92 entries really changed, 31 of
   the ingredients, 24 of them memos that ran again. The true delta of a save is about 0.1% of it.
2. *Why everything differs.* A memo is saved as `{ value, verified_at, revisions }`, where `verified_at`
   is an absolute revision number, and an interned slot as `{ durability, last_interned_at, fields }`,
   an input as per-field changed-at revisions, a tracked struct with its `updated_at`. All 8,693 memos
   in the second state have `verified_at` equal to the save's revision, so the field says nothing; the
   slots' stamps vary (6 for 54K slots, 8 for 25K, `u64::MAX` for the 52 that never expire).
3. *Can a saved dependency set change while its memo does not?* Yes: a query between a memo and its
   inputs can run again and read something new, with the same value. The flattening recomputes every
   set at every save, so a save is always right, but a "this memo did not execute" test is not enough
   to skip one. Dirtiness is therefore **content-addressed first**: a chunk is written when the hash of
   its saved form changed. Generation counters that skip the encoding come later, when the flattening
   they have to trust is shared (workstream B).
4. *Largest ingredients.* By shape: the file texts (12,324 slots, 165 MB), a 117K-memo query, a per-crate
   query with 698 memos at 79 MB, two interned tables. Names are still to be confirmed with the
   per-ingredient size event.
5. *A restore followed by a load writes no unchanged text.* `set_file_text` compares, so it does not;
   only the edited file's text differs between the two states.

**A1. Stable stamps (before the format is split).** When every persisted memo is verified as of the save
revision, which verifying them before the save does, a stamp at or before that revision says nothing a
restore needs: the memos are all valid as of it and whatever a later revision creates is newer. Such a
state is written with every stamp as the first revision (the never-expires value is kept), and a restore
gives the "valid as of the save" stamps the revision it restores: format version 2. A state saved when some
memo is not verified keeps its stamps (version 1, still readable), because a memo an input changed under
must still be found out of date. Tests, in `tests/salsa_stable_save.rs`: after a write to one input, a save
differs from the one before only in that input's slot and the memos that read it (22 differing entries
before, 11 after). Measured on the A0 specimen with a release build: after the same comment edit, 41 of
96,696 entries differ (24 ingredients), where 34,638 did; a change after a canonical restore finds out of date what depends on it through an input,
an interned key and a tracked struct's field; an unverified state is saved with its stamps and not trusted.

**A2. A segmented format.** The state directory holds a manifest (format version, runtime, the ingredient
order, and for each segment its file, length and content hash) and one file per ingredient. Restore reads the
manifest and the segments in the same order as today. Done when every existing saved-state and context-reuse
test passes unchanged and a save followed by a restore followed by a save writes the same bytes, which A1 makes
true.

*Done (2026-11-22).* The Salsa state is a directory: `header` (stamp format and runtime), `manifest` (each
ingredient's index, name and length, in restore order) and `<index>.seg` per ingredient, written one at a time
straight to the file. Salsa gained `save_segments`, `restore_segment` and `finish_segments` beside the single-
stream API. A restore checks each file's length against the manifest before decoding it. A save now reports
every ingredient's bytes under `salsa::state_size` (so the metrics have them on every run), which replaces the
`CORDIAL_STATE_SIZES` switch and its second full encode. Tests: `salsa_segments` (restore reuses, same bytes
after a restore, a change after a restore is found), `saved_state` (a run that changes nothing saves identical
files; truncated, undecodable, missing and damaged states are discarded). Segments are whole ingredients, so
the large ones are still one file each: A4 splits them.

**A3. File texts by reference.** For library and sysroot roots, `FileText` is saved as its length, a content
hash and its path; on restore it is a non-resident `FileContents` that reads the path when asked (the type
already has a re-read source for this: `drop_library_texts`). On the first read the hash is checked, and a
text that differs is written as a change, so what read it is invalidated. Local files keep their text. This
removes about 28% of the bytes on amenable and nearly all the sysroot's, and workstream C relies on it. Tests:
edit a library file between runs and the memos that read it re-run; edit nothing and nothing does.

*Done (2026-11-22).* `FileContents` gained a digest (SHA-256), `attach_source`, `reference`, `is` and a typed
`ColdTextError` under `get` (no source, unreadable, length changed, content changed). Before a save, library
files (dependencies and the sysroot) that are on the disk with the length of their text get their path attached,
and are saved as `{path, len, digest}`; the text stays in memory for the run. A restore makes them cold, and the
load's `set_file_text` compares by digest (`is`), so an unchanged file stays on the disk and a changed one,
even at the same length, is written and invalidates what read it. Measured on the A0 specimen (release): the
saved Salsa state is 46.7 MB where it was 102 MB, and a restored run saves in 0.25 s. Tests:
`file_text_reference` (reference saved, cold on restore, no text in any segment, unchanged load writes nothing,
changed load writes, each error is typed), `cold_texts`, `saved_state`.

**A4. Content-addressed chunks.** Split the large ingredients by id range (for example 4,096 slots or memos).
A save serializes one chunk at a time, hashes it, and writes it only when the hash differs from the manifest's,
linking the previous file otherwise; memory is one chunk. Done when a one-line edit on a fixture rewrites a
small, asserted number of chunks. Ids are assigned mostly in creation order, so an edited file's new values land
in the last chunks.

*A4 done (2026-11-22).* Every ingredient is a top-level map, so the chunking is generic and lives in cordial
(`state_chunks`), not in Salsa: a serializer that takes the ingredient's map and writes its entries into chunks
named for their SHA-256, in `salsa/chunks/`. A chunk ends after an entry whose key hashes (FNV-1a) to one in 64,
or when it passes 1 MiB, so where chunks end depends only on the entries in them and an entry that grows moves
nothing after it. A chunk the previous state had is hard-linked into the new one. A restore reads an ingredient
as a map header and then its chunks in order, which is the encoding it had before chunks, after checking each
chunk's length. Measured on the A0 specimen (release): a cold save is 1,610 chunks, 46.7 MB; after a one-line
edit a save writes 24 chunks, 1.07 MB (2.3%), and links 1,586; a second edit writes 22. Tests: `state_chunks`
(a map of 0 to 70,000 entries reads back, including where the header changes length; the last state's chunks
are linked, none written; a value that grows 50 KB changes at most 3 of 20,000 entries' chunks; large entries
are their own chunks; a non-map is refused and says what it was; a short chunk is found out first),
`saved_state` (an edit changes under a quarter of the chunks).

The save still encodes and hashes every chunk to know that it is unchanged (0.47 s here, which grows with the
state); A5 is what removes that.

**A5. Skip the encoding of clean chunks.** Per-ingredient generations, bumped when a memo is inserted, replaced
or removed and when a struct slot is created, changed or freed, say which chunks cannot have changed, so they are
neither encoded nor hashed. This needs the flattening's sets to be trusted per memo, so it follows workstream B.

**Safety.** Written to a new directory and renamed, with the manifest last, as today; segments that a
link still needs are kept until the rename; a missing, truncated or hash-mismatched segment is a cold
run, never a partial restore. The spill file and the run-in-processes path keep their own state and
are tested with it.

**Checkpoint.** After A4: one amenable run, a cold run and then a one-line edit: bytes written, time of
the save, and the peak.

**Risks.** Slot ids and free lists after a restore must be exactly those of the save, or a delta
refers to the wrong slot; A1's round trip is the test. Dirty tracking that misses a change saves a
stale segment; the test is an equality check of the restored database against a full save on every
fixture that edits. A3's closure rule may make most function ingredients dirty on any edit; A0 says
whether, and then the chunks (A4) carry the saving.

## B. Salsa flattening

**Goal.** The save's transient is a small fraction of the working set, and its CPU is not repeated per
ingredient.

**Causes, from the code and the numbers.** The sets are dense, so a set's size follows its highest
edge number and not how many edges it holds; each ingredient re-walks the shared graph (3.9 times the
distinct work); the verification runs one flattener per thread at once; the serializer keeps every
flattened memo (`pending`) until the ingredient is written.

**B0. Measure.** Add to the `flattened` event the universe size, the number of dense words the
interned sets hold and the entries of `done`, and write a Salsa-level test that builds many sets over a
wide universe and asserts the bytes. This turns the inference above into a number, and it is the
baseline for B1.

**B1. Interval sets.** Replace `Bits` by sorted disjoint runs. Union is a merge, equality and hashing
are over the runs, `runs_of` is the identity, `edges_of` walks runs. A set of a few long runs is a few
words. Property tests against the dense implementation on random sets (union, equality, edges) and
against the saved bytes of every existing persistence test, which must not change: the saved form is
already runs. This is small, independent of A, and can be done when convenient.

**B2. One flatten per save.** The results do not depend on who asks, which the module comment states;
one exception is read in the code: `owner` stops a fingerprint's own dependencies short of the query it
fingerprints for. So the cache is keyed by edge and valid for every owner except the fingerprint
ingredients themselves, which keep a per-owner pass. Use a database-wide edge numbering and renumber to
each ingredient's `universe` when writing, so the saved format does not change. Compute it once,
serially or by a single parallel pass, before the verification parts, which then only read it. Test:
the saved bytes equal the per-ingredient flattening's on every fixture, and the walk counts drop to the
distinct total.

**B3. Stream the memos.** Write an ingredient in two passes, the set table first and then the memos,
flattening each memo again when it is written (a few lookups with B2) instead of holding `pending`. Bound
the parts running at once by a memory budget, not the thread count.

**B4. Restore.** The code counts the edges a restore gives memos of their own, one copy of each shared set
per memo (`flatten.rs`); keep the sets shared. A0 of workstream A says whether this is already so.

**Checkpoint.** After B2: the amenable run's peak against its pre-save working set.

**Risks.** This is the vendored Salsa. Format compatibility is a test, not a hope: byte equality of the
save before and after, on every fixture. The cutoff rule is the subtle part of B2.

## C. Base layers, the sysroot first

**Goal.** What belongs to the toolchain's sysroot is built and saved once, not once per project, and
survives a rebuild of cordial.

**Why layers.** A project's state is a delta over a base: new slots, new memos, and nothing the base
already holds. Workstream A's manifest, segments and dirty tracking are that mechanism, and a base is a
state whose segments are shared. That is why A comes first.

**What a base is.** A state built from a workspace of the sysroot alone, in a fixed order, with its
definition maps built: crates, file ids, source roots and the interned values the sysroot's queries
create. A project run restores it, seeds the VFS with its paths first, and loads the project onto it;
the loader already finds crates again by their data (`crates_map`), which is how a restored database
takes up the workspace today.

**What has to hold, as tests.**
1. *Determinism.* Building the base twice gives the same bytes, with priming's threads off; two
   different projects restore the same base, so the same ids mean the same things.
2. *Closure.* A base memo depends only on base inputs. Check it from the flattened edges: a memo that
   reads anything outside the base stays in the project layer.
3. *Extension.* A project layer adds slots and memos beyond the base's high-water mark and changes none
   of its. A base memo's validity is its base revision, which the library roots' high durability makes
   cheap to check.
4. *Isolation.* Editing a project changes nothing in the base; replacing the base invalidates the
   project layers that name its digest, and they are cold.

**Keys.** The base is keyed by an *engine* fingerprint (a hash of `vendor/` and the rust-analyzer and
Salsa versions, from `build.rs`), the toolchain (`rustc -vV` and the sysroot source's hash) and the
cfg it was built for. The project layer keeps the executable fingerprint, because cordial's own
persisted queries are in it. A rebuild of cordial then discards the project layer and not the base.

**Where it lives and how it is made.** In the shared store, beside the sysroot rustdoc cache, built on
first need under the existing store lock, published by an atomic rename, removed by `cordial clean
--all`. A failed or mismatched base is built again, or the run is cold.

**C0.** Prove 1 and 2 on a fixture by building a "sysroot only" state twice and comparing; measure what
a one-file project's 102 MB is made of. **C1.** Build, save and restore a base, and load a fixture
project onto it; the result equals a cold load. **C2.** Delta-save the project over the base. **C3.**
The engine fingerprint. **C4.** Store lifecycle and the cold fallbacks. **C5.** Measure elicit_doc cold
with and without a base.

**Later, not in this plan.** Registry crates as a layer keyed by their versions and features; it needs
the same machinery and a larger closure test.

**Risks.** Ids interned by threads in a racy order make a base nondeterministic; the base is built on
one thread. The base is restored on every run, so its decode must stay lazy. This does not give cfg
contexts sharing by itself: a context's copy has different data and so different memos. D is separate.

## D. Context and leaf cost

**Goal.** The work of a cfg context is proportional to what the context changes, not to the workspace.

**Measure first (D0).** Per context: the crates whose definition maps and evidence were built, the files
by reason (flipped, leaf, cfg reader, outside the module tree), and the per-crate time. The events exist
for the totals; add the per-crate split, and a unit fixture that counts `crate_local_def_map` executions
per context, so each step below moves a number a test asserts.

**D1. Copies keyed by what they differ in.** A copy's identity is the crate and the part of the context's
cfg change that the crate's files mention (from `gates_of`), not the context's ordinal. Contexts that
differ in atoms a crate never names then share that crate's copy and its memos, and the copy's path
(`/__cordial_context_N/...`) no longer names a run-local number, which also makes its identity stable
across runs. The owner index already allows several owners of a memo; retirement waits for all of them.

**D2. Crate-level aliasing.** A crate whose files are all shared and none of whose local dependencies are
copies is the ordinary crate: no copy, no definition map to build. Its dependents follow to copies only
where a dependency is one.

**D3. The leaf pass.** Leaf files are always copied. Question for a test: could the ordinary graph give
integration-test crates the `test` cfg, so the first leaf context is the ordinary one and later contexts
copy only what a flipped gate reaches? And a cheaper extraction for leaf functions, which need call
targets and not the full evidence.

**D4. Only if D1 to D3 leave a large cost.** Make cfg evaluation a query per atom, so an item tree depends
on the atoms it tests and not on the whole option set. This is a change inside rust-analyzer's name
resolution and the largest item here.

**Checkpoint.** After D2: amenable's contexts and leaf time.

**Risks.** Sharing a copy between contexts changes which context owns a memo; the ownership tests are the
guard. "Unsure" is conservative on purpose; each step that narrows it needs the equivalence tests the
context work already has (findings equal before and after).

## Order and dependencies

```text
A1 -> A2 -> A3 -> A4 -> A5
 \               \
  B0 -> B1 -> B2 -> B3 -> B4        (B1 stands alone; B2 feeds A3 and A5)
            C0 -> C1 -> C2 -> C3 -> C4 -> C5   (needs A1 to A3)
                  D0 -> D1 -> D2 -> D3 -> (D4)  (D0 can start any time)
```

A1 to A4 are the first slice: stable stamps, the segmented format, file texts by reference and content-addressed chunks. B1 is small
enough to take whenever it is convenient. D0 is instrumentation and can run alongside any of them.

## Instrumentation to add, first

The flatten event's universe, dense words and `done` size; bytes written, linked and skipped by a save, and
the segments dirty; base hit and miss and its digest; per-context crates built and files by reason. All as
events the run's metrics keep.

## Out of scope

Registry-crate layers; changing the persistence of Cordial's own queries; the allocator; the cfg-aware
definition maps of D4 unless D1 to D3 leave the cost.

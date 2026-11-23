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

*A5 done, at ingredient grain (2026-11-23).* Each ingredient says whether it has changed since the database was
restored (`unchanged_since_restore`, set wherever a memo is inserted, replaced, evicted, retired or spilled, an input
made or written, an interned value made or replaced, a tracked struct made, updated or deleted; cleared when the
restore ends). A save passes that and whether it writes canonical stamps with each segment; the tool keeps the
manifest of the state it restored from (`Restored::segments`, held by the `SavedState`), and an ingredient that is
unchanged, whose previous entry has the same index, name and file, and whose chunks it can still link, is taken
from there: no encoding, no hashing (`reused` on the `state_size` event, and "analysis state: N of M ingredients
unchanged since the last state, not encoded again"). A tracked function is unchanged by its own flag: a restored,
untouched memo's edges are the sets and own edges it was restored with, so what the flatten would give them is what
was saved, whatever ran elsewhere (non-persisted queries run in every session); the one thing that is not so is the
watermark, which depends on an input's durability, so a write that changes a durability makes every function's
memos suspect. Chunk grain (the plan's wording) is not done: an ingredient is all or nothing, and one memo
inserted in `crate_local_def_map` encodes all of it. B2 changed what had to be true for this to work: the edges an
ingredient saves are now in the order of the edges themselves (ingredient, then key) and not of the order the walk
reached them, since the numbering is global and the order the others were walked in would otherwise change the bytes
of an ingredient that did not change, and A4's chunk linking with it (a set can be a few runs more for it:
`what_the_memos_share_is_saved_as_runs_of_positions_not_as_edges` says 3 where it said 1). Tests: `salsa_unchanged`
(a restored database that changed nothing says every ingredient is unchanged and each encodes to the bytes the
state had; a write changes the input and the memos that ran again and no other; a new interned value changes only
its ingredient), `saved_state` (a save after an edit takes ingredients unchanged and not all, and a run from that
state is the run from a state that encoded every ingredient). Not byte-compared with a full encode: rust-analyzer's
inputs (`Crate`, `SourceRootInput`) hold hash sets, which encode in the order the process gave them, so two
encodings of one value differ (found by trying), and keeping the old bytes is, if anything, the more stable.
Not measured on amenable: the share of the 452 MB that is unchanged after an edit decides the saving, and the
`reused` and `micros` on each `state_size` event will say.

*A5 at chunk grain (2026-11-23).* Inside an ingredient the edit did reach, the memos it did not touch are not
flattened or encoded either. The manifest says how each chunk was cut (entries, first key, a digest of the keys); a
save gives that to Salsa as a plan (`set_chunk_plan`); a memo knows whether it is as a restore made it (its value
still the bytes it was saved as: a memo that is replaced, or whose value is dropped or spilled, is not); and where a
run of memos is a chunk of the plan, all as restored and with the keys the chunk had, the ingredient writes one entry
whose key says `\0reused:<n>` and the chunker links chunk `n` of the last state (a boundary is forced there, which is
only less tidy: a restore concatenates the chunks). What makes a restored memo's bytes the same is its set numbers,
which are positions in the ingredient's table of sets: with a plan the table starts with the sets the last state had,
in the order it had them (an unreferenced one stays, so the table only grows until a save without a plan), then the
new ones. The first chunk (the universe, the sets table, the first memos) is always encoded. The save says how many
chunks it took this way (`chunks_taken`, and in the analysis-state line). Test: `saved_state` (a save after an edit
takes chunks as they were, and a run from that state computes nothing, as does one from a state that encoded
everything). Salsa's side adds a byte to nothing (the flag is in `Spilled`, as `Memo` is held to 48 bytes). Not
measured on amenable.

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

*B0 done (2026-11-22).* The save's `salsa::persistence` event now carries `universe`, `distinct_sets`,
`dense_words`, `dense_bytes`, `widest_set_words` and `kept_results` (the flattener's footprint), and
`tests/salsa_flatten_footprint.rs` pins them on 640 one-edge sets (3,520 words where 640 runs are saved).
Measured on `elicit_doc` (release, 180 MB state, 179,887 persisted memos): the flatteners' sets add up to
29.6 M words, 237 MB, where the same sets saved as runs are 364 K runs, a few MB. One ingredient, `direct_facts`
(6,195 memos), holds 62,768 distinct sets over a universe of 40,377 edges: 19.3 M words, 155 MB, for 198 K runs
(3.2 runs a set, 1.6 MB), and a flattener lives for the whole ingredient. The size of a dense set follows the
universe and the number of sets follows the memos, so the cost grows with their product: `elicit_doc`'s 155 MB
is the small project's number. The queries kept in `done` are 760 K entries across the ingredients, one result
for each query walked. B1 is therefore the first thing to do in B, and it is a large win by itself.

**B1. Interval sets.** Replace `Bits` by sorted disjoint runs. Union is a merge, equality and hashing
are over the runs, `runs_of` is the identity, `edges_of` walks runs. A set of a few long runs is a few
words. Property tests against the dense implementation on random sets (union, equality, edges) and
against the saved bytes of every existing persistence test, which must not change: the saved form is
already runs. This is small, independent of A, and can be done when convenient.

*B1 done (2026-11-22).* `Bits` is `Runs`: sorted, disjoint, non-adjacent `(first, length)` runs, built by a
`RunBuilder` that collects runs and puts them in order and merges them when asked and now and then while it
grows (so joining one large set many times does not hold it many times). Equal sets are equal runs; `runs_of`
is the identity; `edges_of` walks runs. The footprint event now says `held_runs`, `held_bytes` and
`widest_set_runs`. Measured on `elicit_doc` (release, cold): the sets the flatteners hold are 4.1 MB (515 K
runs) where they were 237 MB, and `direct_facts` is 2 MB where it was 155 MB; the save adds 55 MiB to the
run's peak (1.949 to 2.003 GiB). The flatten's time is unchanged (7.4 s summed over the parallel parts): it
is the walk, which B2 is for. Tests: `salsa_flatten_sets` (500 random clustered sets hold exactly their
numbers as the maximal runs a plain implementation gives; equal sets are equal and hash equal whatever order
and repetition; unions; 400 joins of overlapping sets; one number at 4,000,000,000 is one run),
`salsa_flatten_footprint`, and every existing flatten and persistence test unchanged. Saved bytes are the same
by construction (`runs()` is the old maximal-run extraction, `numbers()` ascends as the bits did); a check by
chunk names against the pre-B1 state was inconclusive for an unrelated reason: two cold runs of the *same*
binary differ in about 140 of 1,610 chunk names (parallel id assignment), which is noise to be understood
separately and does not touch restored-then-saved runs (A4's tests).

**Checkpoint, A+B1 (2026-11-22/23, amenable, release, cold).** The first attempt panicked in the save: A3's walk of
the library files asked every file in the VFS for its source root, and the root `Cargo.toml` of a virtual
workspace is in the VFS with none (`FileId 8601`; `drop_library_texts` had the same call, which only runs under
memory pressure). The lookup is now typed (`Files::try_file_source_root` returns `NoSourceRoot` with the file and
its path) and a file with no root is skipped; the save says how many there are and names the first few. The
regression test is a virtual workspace (`cold_texts`), and the diagnostic line confirms such a project has exactly
one unrooted file. Result: the cold run completes; the state is 467 MB (23,515 chunks) with 7,057 library texts
(130 MB) kept as references; the save takes 28 s; the run's peak is 2.8 GB with 2.7 GB resident while it saves,
so the save adds about 0.1 GB, where the last recorded amenable peak was 4.0 GB (before the A and B1 work, with
the other changes of the period). The unchanged second run did not load or restore (it reused the run's cached
IR), so it measured nothing; the edit behaviour is the specimen's (A4). The save is therefore no longer what sets
the peak on amenable, so B2/B3 are now about time (28 s) and not about RSS: what sets the peak is the 2.7 GB
working set the save starts from.

**Decomposition of the working set the save starts from (amenable, cold, release, 2026-11-23).** Peak 3.0 GB,
129 s, save 22.7 s (of which 22.6 s is encoding, flattening included). Resident, in order: the database loaded
0.29 GiB; the workspace loaded 0.85 (+0.55); the definition maps of 158 crates 1.43 (+0.58, 17.7 s); function
evidence primed 2.47 (**+1.04, 18 s, 38% of the working set**); file scans, ordinary projection and the cfg
contexts together +0.2 (the contexts add 0.03); the leaf pass 0.00 (35 s: time, no memory held); the save +0.09.
What the saved state holds, by ingredient: macro expansion (`expand_pure_macro_of` 137 MB, `expand_proc_macro_of`
47 MB, `macrocallid` 31 MB) 45%; crate definition maps (700 memos) 79 MB, 17%; file texts 36 MB; `direct_facts`
24 MB. The flatteners' sets total 14.7 MB. Two things follow. The saved state does not contain the largest step
of the working set: nothing like inference results is in it, so the +1.04 GiB of priming is memory that is held
and not saved, and finding out what it holds is the next measurement. And macro expansion is the largest saved
mass, as it was the largest cost to compute, which is what a base layer (workstream C) shares across crates.

**Two hypotheses about the working set, tried on fixtures and not on large builds (2026-11-23).** (1) *Building a
definition map for every transitive dependency does work nobody asked for.* False. With a strategy option that
builds only the workspace's maps and leaves the dependencies' to be built on demand, a workspace that uses
nothing of the standard library beyond the prelude executes 21 definition maps either way (the 20 of the
sysroot and its own), and one with a registry dependency whose own dependency is used only inside a body (a
vendored directory source, so it is a library crate) executes 23 either way: the evidence follows calls through
library bodies (`collect_reachable`; 340 bodies lowered for a 20-function workspace), so what the closure builds
is what is reached. The option was removed again. (2) *The evidence costs more than a few queries per function.*
False: `tests/evidence_steps.rs` runs a workspace at 20 and at 60 functions and takes the difference of the
queries Salsa executed, by the step that asked: 29 queries for each function added in all, none of any kind
more than 2, and it pins both. What does not grow with the functions is large and is the libraries': about
13,000 impl-signature and attribute queries, and 22,000 attribute queries under `crate_functions`, for a
40-function workspace with the sysroot, all once. So the working set of a project grows with the number of
library crates the evidence reaches and the work for each, not with its functions; an algorithmic win is a
cheaper once-per-library-crate cost or sharing it (workstream C), and not a cheaper function. Allocator
tunables (`arena_max=2`; trim and mmap thresholds) move the resident size of `elicit_doc` by under 3%: it is
live data. Salsa's own count of what the database holds is 142 MB of the 1.96 GiB (what a value owns on the
heap is in neither number), which is why `CORDIAL_FOOTPRINT` reports counts and shallow bytes only.

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

*B2 and B3 done (2026-11-23).* A save keeps one flattener for the whole save (on the database, begun with the
save): every ingredient but the fingerprints (whose walk goes through the query they fingerprint, so they keep a
flattener of their own with its owner) takes it, so a query that two ingredients' memos read is walked once and found
by the next. Edges are numbered across the save; an ingredient writes only the edges its own sets hold, as positions
in that list (`localize`), so the saved format is the one it was and a restore reads it unchanged. The memos are
written in two passes, the distinct sets first and the memos after, each flattened again as it is written (a lookup of
what the first pass found), so the flattened memos are not held between the passes (`pending` is gone). Test:
`salsa_flatten_shared` (two persisted queries over one non-persisted query walk it once between them, 50 walks where
there were 100, and each saves the edges of its own sets), with every salsa, saved-state, chunk and context test
passing. Not measured on amenable: the save's 24 s is where it should show, in the per-ingredient `micros` of the
`salsa::persistence` and `state_size` events. A save's flatten time and the verification's (which keeps a flattener
per thread and is a separate pass) are not the same thing; the verification was left as it was.

*B4 (2026-11-23).* Already so: a restored memo holds one edge for each saved set it refers to, plus its own
(`refer_to_saved`), and the set is checked once for all the memos that share it, so what a restore gives a memo does
not grow with the files of its crate (`restoring_gives_memos_edges_in_proportion_to_the_files_of_the_crate`, passing).
`edges_expanded` counts those edges. Nothing to change.

**Checkpoint.** After B2: the amenable run's peak against its pre-save working set.

**Risks.** This is the vendored Salsa. Format compatibility is a test, not a hope: byte equality of the
save before and after, on every fixture. The cutoff rule is the subtle part of B2.

## C. Base layers, the sysroot first

**Status (2026-11-23): C0 and C1 done at the mechanism level; the discovery that made C work is below.**

*C0.* A crate with no code saves a 40.2 MB state in 72 ingredients, almost all the sysroot (`crate_local_def_map`
23.4 MB, `expand_pure_macro_of` 11.8 MB), and a cold run of it takes 5 s and 0.46 GiB resident (3.9 s for 21
definition maps): that is what a base holds and saves. Two cold runs differ in 4 or 5 ingredients and mildly
(a few chunks of `MacroCallId`, `EditionedFileId` and `FileText`, and the one `SourceRootInput` chunk): racy id
assignment and a hash map's order, so determinism needs the base built on one thread.

*C1.* `SavedState::with_base(dir)` starts a project from a base when its own state is missing or cannot be used;
the base is only read, and a save links the chunks the base already holds. The first test failed in an
instructive way: the project restored from the base and then executed every sysroot query again (22 definition
maps, 35,161 attribute queries, 13,741 impl signatures), the same as cold, while the same project from its own
state executed none. The salsa stale events said why: every memo went stale on its `<watermark>` edge. A persisted
memo summarizes its rarely-changing inputs as one edge, "out of date if any input of this durability or higher
changed", and the watermark was `MEDIUM`, which includes the crate graph and the list of all crates: loading a
project with other crates than the state's wrote `AllCrates.crates` and invalidated everything, for any
different project, and for any edit that adds a crate. The watermark is now `HIGH` (the library's file texts),
the medium inputs are edges of their own, and `source_root_crates`, the one query that reads the list of
crates, is persisted so that memos depend on its answer, which is unchanged for the sysroot's root, and not on
the list. The same project now executes 1 definition map, 30 attribute queries and 6 impl signatures, and the
stale events name exactly what depends on the local root and the crate list. The saved state is the same size
(40.24 MB against 40.20 MB). `tests/base_state.rs` pins: the same graph as a cold load, under a quarter of the
definition maps, more than half of the project's chunks the base's, the base unchanged, and the next run
restoring the project's own state with the identities the first handed out. A crate that already is in the
database and whose data is written, or one that is new, is now an event (`ra_ap_base_db::crates`) with the field
and the crate. What is still to do: C1b, build the base on first need and wire it into the CLI's runs; C3, the
keys; C4, the lifecycle; C5, the measurement on `elicit_doc`, which the watermark change also affects (it
changes what every restored run finds out of date) and which is therefore run once, before anything else.

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

*C1, on a real project (2026-11-23).* `elicit_doc` started from the base still executed all 192 definition maps and
600,732 queries (601,270 without a base): nothing of the sysroot was reused, though the fixtures reused it. A
debug log of only the input writes showed why: 3,190 writes of `FileSourceRootInput.source_root_id`, one for every
sysroot file. A source root's id is its index in a list that `ProjectFolders` sorts by path, so the sysroot's id
moves with every project root and registry crate that sorts before it (registry crates under `~/.cargo` sort
before `~/.rustup`), and a state saved from one project is invalid for the sysroot of any other; the fixtures
only matched because everything in `/tmp` sorts after the toolchain. The sysroot's roots are now put first, in
the order they had, by a stable sort, so their ids are the same for every project. A test with the base and the
project under `/dev/shm`, which sorts before the toolchain, and a workspace of two packages against the base's
one reproduced it (22 definition maps executed) and passes with the fix. The watermark change also needed a
correction found by an existing test: "a crate a tool added" was recognized as crate data that survived the
watermark, which is no longer a property of the crates the loader made, and is now recognized by its durability.
Measured on `elicit_doc` before the root fix: the watermark change cost the saved state 0.6% (182.4 MB against
181.4 MB) and the save no time; the base was made in 7.2 s.

*C1b, C4, C5 (2026-11-23).* `cordial base build` makes the base (a crate that exercises the standard library, saved
from a session of its own) in a child process under the sysroot cache's lock, in `cache/base-state/<key>`, moved
into place when whole; a build prunes the bases of other builds and abandoned build directories; `cordial base
status` says where it is; `CORDIAL_BASE=on` starts a project with no state of its own from it, and it is opt-in
until the default is changed (about twenty tests build fresh stores and several assert on memory). Measured on
`elicit_doc` (release, cold, base made in the run, 10.6 s): queries executed 601,270 without a base and 479,062
with it (-20%); the definition-map phase 11.4 s and 8.3 s; resident after the definition maps 1.02 and 0.78
GiB, after the evidence 1.48 and 1.24, at the end 1.96 and 1.86; peak 2.01 and 1.90 GiB (-0.11); the whole first
run 72 s and 73 s including the base's 10.6 s, so a project after the first in a store is about 10 s faster.
The state is the same size and a save links 1,511 chunks (26 MB) from the base. The effect is a constant, the
sysroot's, about a tenth of this project's working set and less of a larger one's: it does not change what
limits the self-run. *C3 (2026-11-23).* The base now outlives a rebuild of the tool. It holds only what the engine made: a segment is
kept out of it when the ingredient's declaring file is under `src/` (Salsa's `Ingredient::location`, now in
`Segment`), and the check that matters, that rust-analyzer's memos do not depend on the tool's ingredients, holds
(the base tests pass with the tool's segments absent; the cutoff fingerprints the tool can register are not
used in the base's session). A base is keyed by the engine (a hash of `vendor/` and `Cargo.lock`, taken by
`build.rs`), the workload's version, the toolchain, and a digest of how this build numbers the engine's
ingredients, since a state is restored by index. It is restored only when each ingredient it saved has the index,
name and source file it had in the running build: otherwise the run starts cold. Tests: a base has no
ingredient of `src/`; a base whose layout is altered is refused and the run equals the cold run; a copy of the
executable under another path, which is another build to the project layer, uses the base the first made.
The default
changes to on once the tests that build stores say `off`, or the plan's remaining step does.

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

*D0, what is known (2026-11-23).* On amenable, one cfg context and its leaf passes: 6,092 files, of which a
context copies 536 flipped, 285 spine and 2,902 *unsure* and aliases 2,369. The unsure class is the leaf files:
`is_a_leaf_file` counts every test, bench and example as unsure, so it is always copied, which is D3's
question. The leaf pass reads 493 files and 1,427 functions in 32.4 s (66 ms a file, about 23 ms a function,
twice an ordinary function's cost); the context itself executes 536 K queries (a third of the ordinary run's
1.76 M) and adds 0.03 GiB. On a fixture the leaf cost is linear and proportionate (a library and N test files,
run at two sizes: about 124 queries for each file added, two test functions each, dominated by each test's own
macro expansions and the file's item tree and definition map: intrinsic, since an expansion site is unique to
its file). One hypothesis about it is false and is not to be tried again: that the leaf pass is slow because it
reads full evidence, which walks everything a function reaches through the context's copy of the library,
when it uses only a function's own calls (which `direct_facts` holds, identically). Reading calls from
`direct_facts` finds the same leaf functions and executes the same number of queries (163,231 against 163,226,
with the library's chain of 40 functions behind the test): by the time the leaf pass reads, the copy's facts
are primed and the walk is memo lookups. What is not known is the split of the 66 ms a file between the copy
(a new file id, its parse and item tree), the leaf crate's definition map, and its functions' evidence, and how
much of it a leaf in the ordinary graph (D3) would not repeat. The per-crate split D0 asks for is not done.

*D2 done, by the cost model it came from (2026-11-23).* What a context costs was proportional to the workspace,
not to the change: on a fixture of K independent crates with a gate in one, a context built exactly one
definition map for each crate added (1.00 each) and ran about 11 queries for each, because `build_context`
copied every local crate and the harvest and the definition maps followed. A context now copies the crates it
changes (a file under a crate's source directory is flipped, reads the cfg otherwise, or is a leaf) and the
dependencies on the way from one changed crate to another (they define differently through it), and nothing
else: a crate the context does not change is the ordinary crate, a copy that depends on it follows to it, and
its maps are built and shared. Sound by the file-local rule the harvest already has (a function's facts are
re-read only when a flip in its own file touches it), so a function in a crate with no flip is never read in a
context; a crate that depends on a changed crate and is changed by nothing keeps the ordinary crate. A crate
whose root file is not known is taken to be changed. The definition maps under a context are the same at 3
crates and at 9 (and fewer than with every crate copied); on a chain (`top` gated over `mid` over `base`
gated, `other` over `base`) it copies 3 of the 4 and finds the same graph, including the function only the
context has. `copy_every_crate` restores the old behaviour for those tests. Two expectations of
`context_parses` changed and say why: a context that flips nothing in a crate copies nothing of it. Not done:
the leaf files are still copied by every cfg context and read by its harvest (about 16 queries a leaf file,
under 2% on amenable), and D3 (leaf files in the ordinary graph) is untouched.

*D2 on amenable, and a correction (2026-11-23).* The unit tests hold (copies proportional to the change), but on
amenable the first version copied all 110 crates in all four contexts. The cause was not what this section said
earlier ("unsure is the leaf files"): of 1,523 files, 420 contain `cfg_attr` and only 101 are under `tests`,
`benches` or `examples` (27 `include!`, 11 `#[path`, 4 `cfg!`). A `cfg_attr` made its file unsure whatever its
predicate, so nearly every crate was changed. A `cfg_attr` is now evaluated like a gate and changes a file only
where its predicate flips: the leaf contexts went from about 806 unsure files to about 520 and from 641 aliased
to 972, the leaf pass from 32.4 s to 26.7 s (-18%), queries from 1.756 M to 1.740 M, the run from 129 s to
126 s. The copy set is still 110 of 110 in every context, and the remaining reasons are visible in the same
log line: the cfg context flips 524 files of 1,523 (it turns on kani, creusot, verus and a dozen features and
sets `-unix +windows`), which is legitimate work; and the leaf contexts copy the leaf crates, which they exist
to read, and about 380 files with no module in the ordinary build (gated off there, so unsure). The last class
could shrink: such a file changes only where a gate on the path to it flips, which is the flip of the file that
declares it (found as the candidates `files_above` already names, and the crate roots), but a copy set is then
only as small as the crates that are leaf targets and flip-heavy allow, and most of amenable's 110 local
crates are leaf targets (101 leaf files). So this project's context cost is mostly real: D2 does not take it
to a small fraction, and what would (D4, cfg evaluated per atom inside rust-analyzer so that a crate's memos
depend on the atoms it tests) is the largest change in this plan and is not started.

*D1 done, on a fixture (2026-11-23).* A crate's copy under a context is identified by the crate, the part of the
context's change its files name (the atoms of their `cfg` and `cfg_attr` predicates, plus the implicit ones:
`test`, `debug_assertions`, the target's), and the copies it depends on. A context that finds the same key in the
table takes that copy, with its files, and copies only the files of the crates it makes itself (`crates_shared`
in the context's log line). A crate with a `cfg!`, `include!` or `#[path` names more than its predicates say and
takes the whole change. The first version dropped `test` from a crate that never names it and the leaf pass found
no leaves: `#[test]` is read through it, hence the implicit list. On a fixture with three contexts
(`kani`, `creusot`, both, where the first two cannot merge) the contexts build fewer definition maps and find the
same graph (`contexts_that_differ_in_what_a_crate_never_names_share_its_copy`; `do_not_share_context_copies` is
the control). A shared copy and its memos are kept until the pass ends (`release_shared_copies`), not retired with
the context that made it, or the next would build them again; under memory pressure it is retired with its context.
Not measured on amenable or the self-run: the saving there depends on how many contexts differ in atoms a crate
does not name, and what keeping the memos costs.

*D3, first step (2026-11-23).* A file under `tests`, `benches` or `examples` was changed in every context, so every
cfg context copied each leaf crate, which on amenable is most of the 110. It is now changed only in a context with
`test` on (the leaf pass's contexts, which read them); a cfg context treats it as any file, changed where a gate in it
flips or where it has no module in the ordinary build. A fixture with a test file in a crate the context does not
flip copies one crate, not two (`a_cfg_context_does_not_copy_a_crate_for_its_test_files`); I did not run it against
the old rule. Not measured on amenable.

*D3, no-module files (2026-11-23).* A file with no module in the ordinary build was always changed. It is now changed
only where a file that can declare it (the files above it, and its crate's root) has a gate the context flips or reads
the cfg otherwise. A fixture with a hidden file in four crates and two contexts copies three crates in all where the
old rule copied five (`a_file_with_no_module_changes_only_where_the_gate_that_hides_it_flips`, counted from the
rule's own arithmetic, not run against the old code). Not measured on amenable.

*Base default-on (2026-11-23).* A project with no state of its own now starts from the store's base unless
`CORDIAL_BASE=off`; the tests that launch the binary say `off` (each runs in a store of its own, and would build a
base first), and the base tests say `on` as before.

*D4, first finding (2026-11-23).* Where rust-analyzer reads the cfg: about fifteen places read the whole
`krate.cfg_options(db)` input (the def-map collector, item-tree lowering and attributes, `cfg_attr` expansion,
declarative and builtin macros). A context is a copy of the crate with its own cfg input, so the copy's memos are
separate from the ordinary crate's whatever they depend on: evaluating the cfg per atom would not by itself share
anything. It would pay only if a context stopped being a copy and toggled atoms of one crate, with memos that name
the atoms they test, which is a change to how contexts are made and not to the evaluation. One thing it might have
bought is false: a context does not expand the macros of a file it takes as it is
(`a_context_expands_no_macro_in_a_file_it_takes_as_it_is`: the ordinary build expands the calls, the context's
`expand_pure_macro_of` count is 0 at 2 calls and at 12). What a context repeats is therefore the work of the files it
copies and the definition maps of the crates it changes, which is the intrinsic part of D2's finding. Whether the
rest is large enough on amenable to justify toggling in place is not measured.

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

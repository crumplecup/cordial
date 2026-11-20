# Hotpath allocation capability patch

Base: the published `hotpath` 0.31.2 crate, MIT licensed, from
`https://github.com/hotpath-rs/hotpath`, commit
`b8e463528f1d14bcdbb250bcbb536e6b6bb8ec13` (`crates/hotpath`).
The crates.io archive SHA-256 is
`4baf9bc6956d262f9a3d32b8fbfe0982ceed74dfc0d875dc85024c215ffe1b9b`.

This copy includes `src/`, `build.rs`, `Cargo.toml`, and `README.md` from that
archive. Unused upstream binary/test target declarations are removed from the
normalized manifest. Those targets, upstream dev dependencies, and other crate
archive files are not needed by Cordial's library dependency. Upstream runtime
source is unchanged except for the changes recorded in `../hotpath.patch`.

The downstream extension is implemented in Hotpath, not in Cordial:

- `hotpath-live` enables an allocator-instance live/peak requested-byte meter
  without enabling detailed profiling. It uses two `AtomicUsize` fields, no
  dynamic metadata, locks, pointer registry, TLS, or profile guard.
- Both feature configurations use the same allocator implementation.
- Allocation traffic counts only successful requests, including zeroed allocs.
- Successful realloc updates live bytes by its size difference. Traffic still
  counts a full replacement, whether in place or moved. Failure changes neither.
- Live/peak includes recorder work; detailed traffic retains Hotpath's recorder
  exclusions. The live meter has a construction-time zero baseline, no reset,
  and a lifetime peak. Concurrent live and peak reads are independent; in-flight
  calls can be between updates. Quiescent reads reconcile exactly.

`tests/hotpath_heap.rs` in Cordial exercises this API with small local allocator
fixtures, with and without detailed profiling. `tests/hotpath_alloc_contract.rs`
checks traffic and profile-guard independence in fresh test processes. No real
program is profiled by these fixtures.

The versioned `allocation` JSON section exports exact numeric rows and explicit
accounting/disabled states alongside display fields. Lifetime coverage counters
make traffic exclusions and instrumentation losses visible.

This is a local dependency extension, not an upstream release. Retained origins
and full lifecycle integration remain pending in
`docs/planning/memory-measurement.md`. Do not describe this patch as complete RAM
attribution. Keep the patch file synchronized when updating the vendored code.

To reconstruct the source, extract the four paths listed above from the archive
into `vendor/hotpath/`, then apply `vendor/hotpath.patch` with `patch -p1` in that
directory. This provenance note and the upstream `LICENSE.txt` (from the pinned
repository commit) are carried separately.

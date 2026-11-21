# Hotpath allocation consumer contract

Cordial uses published Hotpath 0.31.2 unchanged, with JSON enabled. Allocation
builds enable upstream hotpath-alloc and declare its CountingAllocator once in
src/profiling.rs. Timing-only and disabled builds need no allocation meter.
There is no local Hotpath source, Cargo patch, heap registry or custom profiler.

Function allocation traffic comes from functions_alloc.data. Its profiling_mode
distinguishes alloc-bytes from alloc-count; count mode never becomes byte data.
Exclusive accounting is the default. Cumulative rows overlap and must not be
added. Byte strings preserve upstream precision; rounded KB/MB values are
approximate. Thread frees belong to the freeing thread and do not establish
ownership. RSS and OS high-water remain process measurements, separate from
Rust allocation traffic.

Upstream does not provide the fork's live meter, retained origins or simultaneous
heap composition. These quantities remain unavailable. Historical fork payloads
stay readable with producer provenance and are excluded from upstream traffic
comparisons. Existing operational pressure decisions use process RSS; allocator
retention may keep RSS high after objects are freed.

Capture owners, worker measures, manifests and the existing PerfStore remain the
integration. HOTPATH environment overrides take precedence over guard settings;
a non-JSON or redirected report can leave the expected capture unavailable.
Overrides are recorded rather than suppressed or implemented in a local fork.
Missing, malformed, truncated and disabled data never become zero.

Controlled fixtures under tests/ cover lifecycle, worker boundaries, report
import/storage, mode compatibility and absent evidence. They launch only unit
fixtures, never Cordial on itself. Native allocations, mmap, stacks, dependency
interiors and startup/shutdown remain attribution limits. See
[memory reduction](memory-reduction.md) for application improvements.

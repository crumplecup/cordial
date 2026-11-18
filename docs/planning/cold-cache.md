# Cold cache: trading time for space when RAM is short

When the hot cache (RAM) fills, a run should be able to trade time for space instead of crashing:
every cache it holds goes to a cold tier (disk) where it can, is dropped and computed again where it
cannot, and the run slows down but keeps going. This is a resiliency strategy, not a speed-up. The
worst case is evict and recompute, and the smallest RAM shadow is what remains when everything that
can be cold or recomputed is.

Measurements (`cordial::itemize`, `CORDIAL_MEMORY_LEDGER`, a `hotpath-alloc` profile) are a soft
priority signal only: they order the work, they do not decide whether a cache gets a strategy. Every
type below needs one.

## The ladder

Pressure is the share of the memory limit the process uses (`memory_limit`), judged by the watchdog with
hysteresis. Each level does what the one below does, and more.

| Level | From | The work does |
| --- | --- | --- |
| 1 Soft | 60% | Keeps at most 16 trees and expansions; evicts and spills persisted memos between chunks; priming takes small chunks and stops early. |
| 2 Hard | 75% | A quarter of the workers; drops every value that is not persisted at the end of each chunk; spills everything persisted. |
| 3 Severe | 85% | One worker; the smallest chunk; drops what is not persisted after each chunk. |
| stop | 100% | The watchdog says where the run was and exits. |

## The types

Kinds: **regenerable** (computed again from something smaller: the cold tier is the recompute),
**persistable** (worth a disk copy because it is slow to compute or is the product), **irreducible**
(an input or an id that must stay, so it is made small instead).

| # | Cache | Kind | Strategy | Status |
| --- | --- | --- | --- | --- |
| 1 | Persisted memos (evidence, scans, `file_items`, gates, function lists) | persistable | Spill to `redb` under pressure; restore as bytes and decode on first read | Done (`salsa::spill_persisted`, lazy restore) |
| 2 | Saved state on restore | persistable | Keep memo values as the bytes they were saved as; save them again unchanged | Done |
| 3 | File syntax trees (`parse`) | regenerable | LRU cap, 16 under pressure; dropped with the rest at level 2 | Cap done; drop at level 2 below |
| 4 | Macro-expansion trees (`parse_macro_expansion`), span maps | regenerable | Same; the token tree below is what makes recomputing cheap | Cap done |
| 5 | Macro token trees (declarative, built-in derive and attribute expansions; proc-macro output was already) | persistable | A saved query (`expand_pure_macro_of`): spilled to the cold store, restored as bytes; the tree is rebuilt from it. `include!` and `env!` stay out: they read files and the environment. `macro_arg` is cheap to rebuild and is left to the drop | Done |
| 6 | `ast_id_map`, item trees, attribute flags | regenerable | LRU cap; dropped at level 2 | Cap done where RA marks it |
| 7 | Bodies, signatures, expression scopes, inference results | regenerable | Dropped at the end of each chunk at level 2 and 3 | Stage ends done; chunk ends below |
| 8 | Definition maps | regenerable | Same. Serializing them needs every id they hold persisted | Dropped at stage ends; serializing not planned |
| 9 | File texts (Salsa inputs) | irreducible, re-readable | `FileContents`: a library file's text leaves memory under pressure and is read from its path when asked (a changed length panics); the workspace's files and texts made up in memory stay | Done |
| 10 | Interned data (names, types, item keys, macro call locations) | irreducible | Needs Salsa's interned collection at revision boundaries | Not started |
| 11 | The IR graph's node attributes | persistable | A function's semantic facts (the bulk) are held as bytes, a third or less of the JSON value, decoded on a read and put back to bytes under pressure at the ends of stages; spilling the bytes themselves is the next step | Packed and dropped; spill not started |
| 12 | Workers' in-flight work | transient | Fewer workers as pressure rises | Below |
| 13 | Allocator arenas | overhead | Return freed memory to the system | Tested (`allocator_returns_memory`): glibc gives back memory made on workers and freed on another thread, so no swap is justified; pages left partly used by live objects stay resident, as with any allocator |

## Order of work

1. The ladder itself (levels 2 and 3 above: worker throttling, dropping what is not persisted between
   chunks): it covers rows 3, 4, 6, 7, 8 and 12 at once, so every regenerable cache has the worst-case
   strategy.
2. Macro token trees (5): the expensive recompute.
3. The graph's attributes (11) and file texts (9).
4. Interned data (10) and the allocator (13).

Each lands with a unit test on a toy database or a specimen before it is run on a workload.

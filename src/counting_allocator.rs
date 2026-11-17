//! The system allocator, counting what is held: by each thread (its net bytes), in all, and by
//! the query that was executing when the bytes were allocated.
//!
//! A query's execution is on one thread from start to end, so the difference of a thread's count
//! across it is what the query left behind. Each allocation carries a header with the tag of the
//! query executing then, so that when it is freed, on whichever thread, that query is credited
//! with the release and what each query holds can be read at any moment.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::time::Instant;

/// How many tags there can be: tag 0 is what no query was executing for, and the others are
/// Salsa's ingredient indices plus one.
const TAGS: usize = 16384;

thread_local! {
    static NET: Cell<i64> = const { Cell::new(0) };
    static TAG: Cell<u32> = const { Cell::new(0) };
}

static HELD: [AtomicI64; TAGS] = [const { AtomicI64::new(0) }; TAGS];
static TOTAL: AtomicI64 = AtomicI64::new(0);
static PEAK: AtomicI64 = AtomicI64::new(0);

/// Frees of at least this many bytes are counted by how long they had lived and by who allocated
/// and who freed: small enough that the nodes of a syntax tree are in it.
const PAIRED_FROM: usize = 32;
/// Of those, the pairs are only for memory that lived this long (milliseconds): what is made and
/// freed within a query's execution is not what is held.
const PAIRED_AFTER_MS: u32 = 100;
/// Upper bounds, in milliseconds, of the ages memory is counted by when it is freed; the last
/// bucket is every age above.
const AGES_MS: [u32; 4] = [10, 100, 1000, 10_000];

static START: OnceLock<Instant> = OnceLock::new();
static LIVED: [[AtomicI64; 5]; TAGS] = [const { [const { AtomicI64::new(0) }; 5] }; TAGS];

/// Milliseconds since the first allocation counted.
fn now_ms() -> u32 {
    u32::try_from(START.get_or_init(Instant::now).elapsed().as_millis()).unwrap_or(u32::MAX)
}
/// A table of (allocating tag, freeing tag) pairs, open-addressed so that counting never allocates.
const PAIRS: usize = 16384;
static PAIR_KEY: [AtomicU64; PAIRS] = [const { AtomicU64::new(0) }; PAIRS];
static PAIR_BYTES: [AtomicI64; PAIRS] = [const { AtomicI64::new(0) }; PAIRS];

fn pair_add(allocated: u32, freed: u32, bytes: usize) {
    let key = ((u64::from(allocated) << 32) | u64::from(freed)) + 1;
    let mut at = (key.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 40) as usize % PAIRS;
    for _ in 0..64 {
        let found = PAIR_KEY[at].load(Ordering::Relaxed);
        if found == key
            || (found == 0
                && PAIR_KEY[at]
                    .compare_exchange(0, key, Ordering::Relaxed, Ordering::Relaxed)
                    .map_or_else(|other| other == key, |_| true))
        {
            PAIR_BYTES[at].fetch_add(i64::try_from(bytes).unwrap_or(i64::MAX), Ordering::Relaxed);
            return;
        }
        at = (at + 1) % PAIRS;
    }
}

/// Counts for the program's allocator.
pub struct Counting;

fn add(bytes: usize, grows: bool) {
    let bytes = i64::try_from(bytes).unwrap_or(i64::MAX);
    let signed = if grows { bytes } else { -bytes };
    // Not available while the thread is being torn down: nothing to count then.
    let _ = NET.try_with(|net| net.set(net.get() + signed));
    let now = TOTAL.fetch_add(signed, Ordering::Relaxed) + signed;
    if grows {
        PEAK.fetch_max(now, Ordering::Relaxed);
    }
}

fn current_tag() -> u32 {
    TAG.try_with(Cell::get).unwrap_or(0)
}

/// What to ask of the system for a request, and where the caller's bytes start in it: after a
/// header that is as large as the alignment asks, and never under 16 bytes.
fn padded(layout: Layout) -> Option<(Layout, usize)> {
    let pad = layout.align().max(16);
    Layout::from_size_align(layout.size().checked_add(pad)?, layout.align())
        .ok()
        .map(|layout| (layout, pad))
}

/// Writes the tag of the executing query before the bytes the caller gets, and counts them for it.
fn tagged(real: *mut u8, pad: usize, size: usize) -> *mut u8 {
    if real.is_null() {
        return real;
    }
    let tag = current_tag();
    // SAFETY: `pad` is at least 16, so `pad - 4` is inside the allocation, and it is aligned for
    // a `u32` because `real` is aligned to at least 16 and `pad` is a multiple of 4.
    unsafe {
        let user = real.add(pad);
        user.sub(4).cast::<u32>().write(tag);
        user.sub(8).cast::<u32>().write(now_ms());
        HELD[tag as usize % TAGS]
            .fetch_add(i64::try_from(size).unwrap_or(i64::MAX), Ordering::Relaxed);
        user
    }
}

// SAFETY: every method asks the system allocator for the request with a header in front and gives
// back what is after it, and frees by the address it was given, less the header.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let Some((asked, pad)) = padded(layout) else {
            return std::ptr::null_mut();
        };
        add(layout.size(), true);
        // SAFETY: `asked` is a valid layout, as made by `padded`.
        tagged(unsafe { System.alloc(asked) }, pad, layout.size())
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let Some((asked, pad)) = padded(layout) else {
            return;
        };
        add(layout.size(), false);
        // SAFETY: `ptr` came from `alloc` with this layout, so the header is before it.
        unsafe {
            let tag = ptr.sub(4).cast::<u32>().read();
            HELD[tag as usize % TAGS].fetch_sub(
                i64::try_from(layout.size()).unwrap_or(i64::MAX),
                Ordering::Relaxed,
            );
            if layout.size() >= PAIRED_FROM {
                let age = now_ms().wrapping_sub(ptr.sub(8).cast::<u32>().read());
                let bucket = AGES_MS.iter().position(|bound| age < *bound).unwrap_or(4);
                LIVED[tag as usize % TAGS][bucket].fetch_add(
                    i64::try_from(layout.size()).unwrap_or(i64::MAX),
                    Ordering::Relaxed,
                );
                if age >= PAIRED_AFTER_MS {
                    pair_add(tag, current_tag(), layout.size());
                }
            }
            System.dealloc(ptr.sub(pad), asked);
        }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let Some((asked, pad)) = padded(layout) else {
            return std::ptr::null_mut();
        };
        add(layout.size(), true);
        // SAFETY: `asked` is a valid layout, as made by `padded`.
        tagged(unsafe { System.alloc_zeroed(asked) }, pad, layout.size())
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the new layout has the old one's alignment, and a size the caller vouches for.
        let new_layout = unsafe { Layout::from_size_align_unchecked(new_size, layout.align()) };
        // SAFETY: as the caller's contract for `alloc`.
        let fresh = unsafe { self.alloc(new_layout) };
        if !fresh.is_null() {
            // SAFETY: both are valid for the smaller of the two sizes and do not overlap.
            unsafe {
                std::ptr::copy_nonoverlapping(ptr, fresh, layout.size().min(new_size));
                self.dealloc(ptr, layout);
            }
        }
        fresh
    }
}

/// The bytes the calling thread has allocated and not freed.
pub fn net_bytes() -> i64 {
    NET.try_with(Cell::get).unwrap_or(0)
}

/// Say which query the calling thread is executing for, as the tag its allocations carry from
/// now on; gives the tag it had.
pub fn set_tag(tag: u32) -> u32 {
    TAG.try_with(|current| current.replace(tag)).unwrap_or(0)
}

/// What every thread together holds.
pub fn held_in_all() -> i64 {
    TOTAL.load(Ordering::Relaxed)
}

/// Bytes freed, by the tag of the query that allocated them and of the one executing when they
/// were freed (0 for neither), for the frees of 32 bytes or more that lived long enough to be held.
pub fn freed_pairs() -> Vec<(u32, u32, i64)> {
    (0..PAIRS)
        .filter_map(|at| {
            let key = PAIR_KEY[at].load(Ordering::Relaxed);
            let bytes = PAIR_BYTES[at].load(Ordering::Relaxed);
            (key != 0).then(|| {
                (
                    ((key - 1) >> 32) as u32,
                    ((key - 1) & 0xFFFF_FFFF) as u32,
                    bytes,
                )
            })
        })
        .collect()
}

/// Bytes freed by the tag that allocated them and how long they lived: under 10 ms, 100 ms, 1 s,
/// 10 s, and longer; for the frees of 32 bytes or more.
pub fn freed_by_age() -> Vec<(u32, [i64; 5])> {
    LIVED
        .iter()
        .enumerate()
        .filter_map(|(tag, buckets)| {
            let counts = buckets
                .each_ref()
                .map(|bucket| bucket.load(Ordering::Relaxed));
            counts
                .iter()
                .any(|count| *count != 0)
                .then_some((tag as u32, counts))
        })
        .collect()
}

/// The most every thread together has held at once.
pub fn peak_held() -> i64 {
    PEAK.load(Ordering::Relaxed)
}

/// What each tag holds that is not zero.
pub fn held_by_tag() -> Vec<(u32, i64)> {
    HELD.iter()
        .enumerate()
        .map(|(tag, held)| (tag as u32, held.load(Ordering::Relaxed)))
        .filter(|(_, held)| *held != 0)
        .collect()
}

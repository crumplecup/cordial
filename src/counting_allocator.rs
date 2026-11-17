//! The system allocator, counting what is held: by each thread (its net bytes), in all, and by
//! the query that was executing when the bytes were allocated.
//!
//! A query's execution is on one thread from start to end, so the difference of a thread's count
//! across it is what the query left behind. Each allocation carries a header with the tag of the
//! query executing then, so that when it is freed, on whichever thread, that query is credited
//! with the release and what each query holds can be read at any moment.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicI64, Ordering};

/// How many tags there can be: tag 0 is what no query was executing for, and the others are
/// Salsa's ingredient indices plus one.
const TAGS: usize = 16384;

thread_local! {
    static NET: Cell<i64> = const { Cell::new(0) };
    static TAG: Cell<u32> = const { Cell::new(0) };
}

static HELD: [AtomicI64; TAGS] = [const { AtomicI64::new(0) }; TAGS];
static TOTAL: AtomicI64 = AtomicI64::new(0);

/// Counts for the program's allocator.
pub struct Counting;

fn add(bytes: usize, grows: bool) {
    let bytes = i64::try_from(bytes).unwrap_or(i64::MAX);
    let signed = if grows { bytes } else { -bytes };
    // Not available while the thread is being torn down: nothing to count then.
    let _ = NET.try_with(|net| net.set(net.get() + signed));
    TOTAL.fetch_add(signed, Ordering::Relaxed);
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

/// What each tag holds that is not zero.
pub fn held_by_tag() -> Vec<(u32, i64)> {
    HELD.iter()
        .enumerate()
        .map(|(tag, held)| (tag as u32, held.load(Ordering::Relaxed)))
        .filter(|(_, held)| *held != 0)
        .collect()
}

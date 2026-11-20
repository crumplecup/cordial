//! Bounded allocation-origin retention, owned by the allocator instance.
//! No pointer registry or dynamic metadata allocation is used.
use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use crate::{HeapSnapshot, OriginKind, RetainedOrigin};

pub const MAX_ALLOCATION_ORIGINS: usize = 1024;
const RESERVED: usize = 4;
const SLOTS: usize = MAX_ALLOCATION_ORIGINS + RESERVED;

#[derive(Clone, Copy)]
pub(crate) enum Source {
    Unscoped,
    Recorder,
    TlsUnavailable,
    Function(&'static str),
}

#[derive(Clone, Copy)]
struct Slot {
    name: Option<&'static str>,
    live: usize,
    blocks: usize,
    peak: usize,
}
impl Slot {
    const EMPTY: Self = Self {
        name: None,
        live: 0,
        blocks: 0,
        peak: 0,
    };
}

struct State {
    capacity: usize,
    used: usize,
    slots: [Slot; SLOTS],
    live: usize,
    peak: usize,
    blocks: usize,
    headers: usize,
    allocated: u64,
    freed: u64,
    allocations: u64,
    reallocations: u64,
    deallocations: u64,
    alloc_failures: u64,
    realloc_failures: u64,
    overflow_allocations: u64,
    overflow_bytes: u64,
}

pub(crate) struct Registry {
    locked: AtomicBool,
    state: UnsafeCell<State>,
}
// SAFETY: every access to state holds the spinlock. No inner allocator call,
// TLS access, formatting, heap allocation or user callback runs under the lock.
unsafe impl Sync for Registry {}

struct Unlock<'a>(&'a AtomicBool);
impl Drop for Unlock<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl Registry {
    pub(crate) const fn new(capacity: usize) -> Self {
        Self {
            locked: AtomicBool::new(false),
            state: UnsafeCell::new(State {
                capacity: if capacity > MAX_ALLOCATION_ORIGINS {
                    MAX_ALLOCATION_ORIGINS
                } else {
                    capacity
                },
                used: 0,
                slots: [Slot::EMPTY; SLOTS],
                live: 0,
                peak: 0,
                blocks: 0,
                headers: 0,
                allocated: 0,
                freed: 0,
                allocations: 0,
                reallocations: 0,
                deallocations: 0,
                alloc_failures: 0,
                realloc_failures: 0,
                overflow_allocations: 0,
                overflow_bytes: 0,
            }),
        }
    }

    fn with<R>(&self, f: impl FnOnce(&mut State) -> R) -> R {
        while self
            .locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            std::hint::spin_loop();
        }
        let _unlock = Unlock(&self.locked);
        // SAFETY: exclusive lock protects state, and the borrow cannot escape f.
        f(unsafe { &mut *self.state.get() })
    }

    pub(crate) fn allocate(
        &self,
        source: Source,
        size: usize,
        header: usize,
        live: &AtomicUsize,
        peak: &AtomicUsize,
    ) -> usize {
        self.with(|s| {
            let id = match source {
                Source::Unscoped => 0,
                Source::Recorder => 1,
                Source::TlsUnavailable => 2,
                Source::Function(name) => {
                    if let Some(i) = s.slots[RESERVED..RESERVED + s.used]
                        .iter()
                        .position(|slot| slot.name == Some(name))
                    {
                        RESERVED + i
                    } else if s.used < s.capacity {
                        let i = RESERVED + s.used;
                        s.slots[i].name = Some(name);
                        s.used += 1;
                        i
                    } else {
                        s.overflow_allocations += 1;
                        s.overflow_bytes += size as u64;
                        3
                    }
                }
            };
            let slot = &mut s.slots[id];
            slot.live += size;
            slot.blocks += 1;
            slot.peak = slot.peak.max(slot.live);
            s.live += size;
            s.peak = s.peak.max(s.live);
            s.blocks += 1;
            s.headers += header;
            s.allocated += size as u64;
            s.allocations += 1;
            live.store(s.live, Ordering::Relaxed);
            peak.store(s.peak, Ordering::Relaxed);
            id
        })
    }

    pub(crate) fn free(
        &self,
        id: usize,
        size: usize,
        header: usize,
        live: &AtomicUsize,
        peak: &AtomicUsize,
    ) {
        self.with(|s| {
            s.slots[id].live -= size;
            s.slots[id].blocks -= 1;
            s.live -= size;
            s.blocks -= 1;
            s.headers -= header;
            s.freed += size as u64;
            s.deallocations += 1;
            live.store(s.live, Ordering::Relaxed);
            peak.store(s.peak, Ordering::Relaxed);
        });
    }

    pub(crate) fn resize(
        &self,
        id: usize,
        old: usize,
        new: usize,
        live: &AtomicUsize,
        peak: &AtomicUsize,
    ) {
        self.with(|s| {
            let slot = &mut s.slots[id];
            slot.live = slot.live - old + new;
            slot.peak = slot.peak.max(slot.live);
            s.live = s.live - old + new;
            s.peak = s.peak.max(s.live);
            s.allocated += new as u64;
            s.freed += old as u64;
            s.reallocations += 1;
            live.store(s.live, Ordering::Relaxed);
            peak.store(s.peak, Ordering::Relaxed);
        });
    }

    pub(crate) fn failed(&self, realloc: bool) {
        self.with(|s| {
            if realloc {
                s.realloc_failures += 1;
            } else {
                s.alloc_failures += 1;
            }
        });
    }

    pub(crate) fn snapshot(&self) -> HeapSnapshot {
        // Capacity is allocated before locking; copying slots cannot allocate.
        // Recorder suppression ensures the snapshot's own storage is explicit.
        let _suspend = crate::lib_on::SuspendAllocTracking::new();
        let mut rows = Vec::with_capacity(SLOTS);
        let mut snapshot = self.with(|s| {
            rows.extend_from_slice(&s.slots[..RESERVED + s.used]);
            HeapSnapshot {
                live_bytes: s.live,
                peak_live_bytes: s.peak,
                live_allocations: s.blocks,
                allocated_bytes: s.allocated,
                freed_bytes: s.freed,
                successful_allocations: s.allocations,
                successful_reallocations: s.reallocations,
                deallocations: s.deallocations,
                allocation_failures: s.alloc_failures,
                reallocation_failures: s.realloc_failures,
                origin_capacity: s.capacity,
                registered_origins: s.used,
                overflow_allocations: s.overflow_allocations,
                overflow_allocated_bytes: s.overflow_bytes,
                registry_bytes: std::mem::size_of::<Self>(),
                live_header_bytes: s.headers,
                origins: Vec::new(),
            }
        });
        snapshot.origins = rows
            .into_iter()
            .enumerate()
            .map(|(id, slot)| RetainedOrigin {
                id,
                kind: match id {
                    0 => OriginKind::Unscoped,
                    1 => OriginKind::Recorder,
                    2 => OriginKind::TlsUnavailable,
                    3 => OriginKind::Overflow,
                    _ => OriginKind::Function,
                },
                name: slot.name.map(str::to_owned),
                live_bytes: slot.live,
                live_allocations: slot.blocks,
                peak_live_bytes: slot.peak,
            })
            .collect();
        snapshot
    }
}

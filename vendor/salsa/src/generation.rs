//! Whether an ingredient has been written to since the database was restored.
//!
//! A save can leave an ingredient's encoding to what the state it was restored from holds, when nothing
//! has changed what it would encode: a memo inserted, replaced or dropped, a struct made, written or freed.
//! Each ingredient says so with one of these, which is touched wherever it changes (and is only ever
//! set, until a restore ends and clears it).

use std::sync::atomic::{AtomicBool, Ordering};

/// Set when the ingredient changes.
#[derive(Debug, Default)]
pub(crate) struct Touched(AtomicBool);

impl Touched {
    /// The ingredient changed.
    #[inline]
    pub(crate) fn touch(&self) {
        // Read first: most writes are after the first, and a shared store makes the line bounce.
        if !self.0.load(Ordering::Relaxed) {
            self.0.store(true, Ordering::Relaxed);
        }
    }

    /// Whether it has changed since the last [`Self::clear`].
    pub(crate) fn is_touched(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    /// What the ingredient holds now is what a state it was restored from holds.
    pub(crate) fn clear(&self) {
        self.0.store(false, Ordering::Relaxed);
    }
}

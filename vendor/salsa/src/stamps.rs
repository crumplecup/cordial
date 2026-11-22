//! How the revisions in a saved state are written, so that a state saved again is the same where
//! nothing changed.
//!
//! A saved memo says the revision it was last verified at, a slot the revision it was last interned
//! or changed at, an input the revision each field changed at. They move on with every revision, so
//! a state saved after any other revision differs from the one before in every entry, though almost
//! none of what they hold changed: after one comment line was added, 99.7% of the entries that
//! differed differed only in a stamp. A save cannot then tell what changed, still less write only that.
//!
//! When every persisted memo is verified as of the revision of the save, which verifying them before
//! the save does, a stamp at or before that revision says nothing a restore needs: the memos are all
//! valid as of it, and what a later revision creates is newer than any of them. Such a state is saved
//! with every stamp as the first revision, except the revision that marks a value as never expiring,
//! and a restore gives the stamps that stand for "valid as of the save" the revision it restores. A
//! state saved when some memo is not verified keeps its stamps, as it always did: a memo an input
//! changed under must still be found out of date.

use std::cell::Cell;

use crate::Revision;

thread_local! {
    /// Whether the ingredients being written are written with their stamps as the first revision.
    static CANONICAL: Cell<bool> = const { Cell::new(false) };
    /// The revision a restore gives the stamps that stand for "valid as of the save", when the state
    /// being restored was written with them as the first revision.
    static RESTORED: Cell<Option<usize>> = const { Cell::new(None) };
}

/// Writes ingredients with canonical stamps while it lives.
pub(crate) struct Canonical {
    before: bool,
}

impl Canonical {
    pub(crate) fn enter(on: bool) -> Self {
        Self {
            before: CANONICAL.with(|canonical| canonical.replace(on)),
        }
    }
}

impl Drop for Canonical {
    fn drop(&mut self) {
        CANONICAL.with(|canonical| canonical.set(self.before));
    }
}

/// The stamp a save writes for `value`, a revision as a number (0 stands for none, and the largest
/// number for a value that never expires: both are kept).
pub(crate) fn stamp(value: usize) -> usize {
    if value == 0 || value == usize::MAX || !CANONICAL.with(Cell::get) {
        value
    } else {
        1
    }
}

/// Restores the stamps of a state written with canonical stamps while it lives.
pub(crate) struct Restoring {
    before: Option<usize>,
}

impl Restoring {
    /// `current` is the revision the state was saved at, or `None` for a state with its own stamps.
    pub(crate) fn enter(current: Option<Revision>) -> Self {
        Self {
            before: RESTORED.with(|restored| restored.replace(current.map(Revision::as_usize))),
        }
    }
}

impl Drop for Restoring {
    fn drop(&mut self) {
        RESTORED.with(|restored| restored.set(self.before));
    }
}

/// The revision a stamp that stands for "valid as of the save" is restored as, when the state being
/// restored was written with canonical stamps.
pub(crate) fn restored_current() -> Option<Revision> {
    RESTORED.with(Cell::get).map(Revision::from)
}

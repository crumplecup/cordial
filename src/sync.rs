//! Locks and channels, from one place so hotpath can see them.
//!
//! With the `hotpath` feature these are hotpath's instrumented wrappers, and
//! every lock reports how long it was waited for and held, every channel how deep
//! its queue ran. Without it they are the standard library's own types, and the
//! macros expand to the plain constructors, so nothing changes at run time. A lock
//! or a channel is created with [`mutex!`] and [`channel!`] and named, so a
//! profile says which one it is; the types come from here too, so a field's type
//! is the same in both builds.

pub use hotpath::wrap::std::sync::{Mutex, MutexGuard, mpsc};

/// A [`Mutex`] holding `$value`, named `$label` in profiles.
///
/// The label is passed to hotpath as a block expression, not a literal. Given a
/// literal, hotpath checks that labels are unique with an `export_name`
/// attribute, which the crate's `#![forbid(unsafe_code)]` rejects; the block
/// form is the same label without that check. Keep labels distinct by hand: the
/// profile shows two locks with one name as one.
macro_rules! mutex {
    ($value:expr, $label:literal) => {
        hotpath::mutex!(std::sync::Mutex::new($value), label = { $label })
    };
}

/// An unbounded channel of `$item`, named `$label` in profiles. The label is
/// passed as a block expression for the reason given at [`mutex!`].
macro_rules! channel {
    ($item:ty, $label:literal) => {
        hotpath::channel!(std::sync::mpsc::channel::<$item>(), label = { $label })
    };
}

pub(crate) use channel;
pub(crate) use mutex;

//! Where a command's output goes and what its run decided about reuse.

mod notes;
mod output;

pub use notes::{RunNotes, last_events, take as take_notes};
pub(crate) use notes::{cache_hit, event, loaded};
pub use output::{Out, stderr, stdout};

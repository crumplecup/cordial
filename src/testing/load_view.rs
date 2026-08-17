//! Test helpers for constructing loader views outside the session hook loop.

use crate::RustdocLoadView;
use crate::rustdoc::RustdocInventory;

/// Build a rustdoc load view from a parsed inventory (integration tests / oracles).
pub fn rustdoc_load_view(inventory: RustdocInventory) -> RustdocLoadView {
    RustdocLoadView::from_inventory(inventory)
}

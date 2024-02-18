pub mod host;
pub mod guest;
pub mod memory;
pub mod polite;

/// The `prelude` module contains re-exports of the primary structs and functions in the library
/// for easier use.
pub mod prelude {
    pub use crate::host::{Counsel, Host, Improv, Posture, Recall};
    #[cfg(feature = "serde")]
    #[cfg(feature = "uuid")]
    pub use crate::guest::Guest;
    pub use crate::memory::Memorable;
    pub use crate::polite::{Polite, FauxPas};
}

pub mod guest;
pub mod host;
pub mod memory;
pub mod polite;

/// The `prelude` module contains re-exports of the primary structs and functions in the library
/// for easier use.
pub mod prelude {
    #[cfg(feature = "serde")]
    #[cfg(feature = "uuid")]
    pub use crate::guest::Guest;
    pub use crate::host::{Counsel, Host, Improv, Pass, Posture, Recall};
    pub use crate::memory::Memorable;
    pub use crate::polite::{FauxPas, Polite};
}

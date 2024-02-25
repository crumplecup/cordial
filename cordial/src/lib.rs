pub mod guest;
pub mod host;
pub mod memory;
pub mod polite;

/// The `prelude` module contains re-exports of the primary structs and functions in the library
/// for easier use.
pub mod prelude {
    #[cfg(feature = "serial")]
    #[cfg(feature = "uuids")]
    pub use crate::guest::Guest;
    #[cfg(feature = "route")]
    #[cfg_attr(docsrs, doc(cfg(feature = "route")))]
    pub use crate::host::Counsel;
    #[cfg(feature = "route")]
    #[cfg_attr(docsrs, doc(cfg(feature = "route")))]
    #[cfg(feature = "sql")]
    #[cfg_attr(docsrs, doc(cfg(feature = "sql")))]
    pub use crate::host::Host;
    #[cfg(feature = "improv")]
    #[cfg_attr(docsrs, doc(cfg(feature = "improv")))]
    pub use crate::host::Improv;
    #[cfg(feature = "improv")]
    #[cfg_attr(docsrs, doc(cfg(feature = "improv")))]
    #[cfg(feature = "serial")]
    #[cfg_attr(docsrs, doc(cfg(feature = "serial")))]
    pub use crate::host::Pass;
    #[cfg(feature = "secret")]
    #[cfg_attr(docsrs, doc(cfg(feature = "secret")))]
    pub use crate::host::Posture;
    #[cfg(feature = "sql")]
    #[cfg_attr(docsrs, doc(cfg(feature = "sql")))]
    pub use crate::host::Recall;
    #[cfg(feature = "memory")]
    #[cfg_attr(docsrs, doc(cfg(feature = "memory")))]
    #[cfg(feature = "uuids")]
    #[cfg_attr(docsrs, doc(cfg(feature = "uuids")))]
    pub use crate::memory::Memorable;
    pub use crate::polite::{FauxPas, Polite};
}

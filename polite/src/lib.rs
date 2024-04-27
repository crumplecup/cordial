pub mod polite;

pub use polite::{FauxPas, Polite};

#[cfg(feature = "parse")]
#[cfg_attr(docsrs, doc(cfg(feature = "parse")))]
pub use polite::NomDescript;

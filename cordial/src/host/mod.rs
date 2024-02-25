//! The `host` module holds structs related to managing services for a guest ([`crate::guest::Guest`]), including
//! database registration and route handling.
pub mod counsel;
pub mod eponym;
pub mod improv;
pub mod posture;
pub mod recall;

#[cfg(feature = "route")]
#[cfg_attr(docsrs, doc(cfg(feature = "route")))]
pub use counsel::Counsel;
#[cfg(feature = "route")]
#[cfg_attr(docsrs, doc(cfg(feature = "route")))]
#[cfg(feature = "sql")]
#[cfg_attr(docsrs, doc(cfg(feature = "sql")))]
pub use eponym::Host;
#[cfg(feature = "improv")]
#[cfg_attr(docsrs, doc(cfg(feature = "improv")))]
pub use improv::Improv;
#[cfg(feature = "improv")]
#[cfg_attr(docsrs, doc(cfg(feature = "improv")))]
#[cfg(feature = "serial")]
#[cfg_attr(docsrs, doc(cfg(feature = "serial")))]
pub use improv::Pass;
#[cfg(feature = "secret")]
#[cfg_attr(docsrs, doc(cfg(feature = "secret")))]
pub use posture::Posture;
#[cfg(feature = "sql")]
#[cfg_attr(docsrs, doc(cfg(feature = "sql")))]
pub use recall::Recall;

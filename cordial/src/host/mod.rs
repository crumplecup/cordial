//! The `host` module holds structs related to managing services for a guest ([`crate::guest::Guest`]), including
//! database registration and route handling.
pub mod bearing;
pub mod counsel;
pub mod improv;
pub mod posture;
pub mod recall;

pub use bearing::Bearing;
pub use counsel::Counsel;
pub use improv::Improv;
pub use posture::Posture;
pub use recall::Recall;

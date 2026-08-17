//! Downstream templates for the three plugin kinds cordial expects.
//!
//! | Kind | Type | Supertrait |
//! | --- | --- | --- |
//! | Quality family | [`ACME_STYLE`] | [`Plugin`](cordial::Plugin) only (`StaticPlugin`) |
//! | Coverage | [`AcmeApiCoverage`] | [`Coverage`](cordial::Coverage) |
//! | Error handling | [`AcmeErrorHandling`] | [`ErrorHandling`](cordial::ErrorHandling) |
//!
//! Register with [`SessionBuilder::register_plugin`](cordial::SessionBuilder::register_plugin).
//! See `docs/planning/custom-plugin-example.md` in the cordial repo.

mod coverage;
mod error_handling;
mod quality;

pub use coverage::{ACME_API_COVERAGE, AcmeApiCoverage, DisplayRequirement};
pub use error_handling::{ACME_ERROR_HANDLING, AcmeErrorHandling, AcmeErrorPolicy};
pub use quality::{ACME_STYLE, TODO_ETIQUETTE};

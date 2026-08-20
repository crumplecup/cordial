//! Downstream templates for the three plugin kinds cordial expects.
//!
//! | Kind | Type | Supertrait |
//! | --- | --- | --- |
//! | Quality family | [`ACME_STYLE`] | [`Plugin`](cordial::Plugin) only (`StaticPlugin`) |
//! | Coverage | [`AcmeApiCoverage`] | [`Coverage`](cordial::Coverage) |
//! | Error handling | [`AcmeErrorHandling`] | [`ErrorHandling`](cordial::ErrorHandling) |
//!
//! Register with [`SessionBuilder::register_plugin`](cordial::SessionBuilder::register_plugin).
//! See `docs/planning/custom-plugin-example.md`.
//!
//! ```text
//! cargo run --example custom_plugins --features impl_coverage
//! ```

mod coverage;
mod error_handling;
mod quality;

use cordial::Plugin;
use coverage::ACME_API_COVERAGE;
use error_handling::ACME_ERROR_HANDLING;
use quality::ACME_STYLE;

fn main() {
    let plugins: [&dyn Plugin; 3] = [&ACME_STYLE, &ACME_API_COVERAGE, &ACME_ERROR_HANDLING];
    for plugin in plugins {
        println!(
            "{} ({:?}): {}",
            plugin.id(),
            plugin.category(),
            plugin.name()
        );
    }
}

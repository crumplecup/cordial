//! Canonical error-analysis payloads from the native database match the
//! compatibility loader, including virtual revisions and source-free replay,
//! sharing one semantic workspace load.
#![cfg(any(
    feature = "error_sites",
    feature = "error_chain",
    feature = "internal_error_chain"
))]
#[path = "../support/error_sites_default.rs"]
mod default_sites;
#[path = "../support/error_sites_projection.rs"]
mod projection;
#[path = "../support/shared_workspace.rs"]
mod shared;

#[cfg(feature = "internal_error_chain")]
mod architecture;
#[cfg(feature = "error_chain")]
mod chains;
#[cfg(feature = "internal_error_chain")]
mod compliance;
#[cfg(feature = "internal_error_chain")]
mod signatures;
#[cfg(feature = "error_sites")]
mod sites;
#[cfg(feature = "internal_error_chain")]
mod types;

use shared::Scenario;

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn native_error_payloads_match_the_compatibility_loader() -> miette::Result<()> {
    cordial::init_tracing();
    shared::run(&[
        #[cfg(feature = "internal_error_chain")]
        Scenario::new(
            architecture::PACKAGE,
            architecture::write,
            architecture::check,
        ),
        #[cfg(feature = "error_chain")]
        Scenario::new(chains::PACKAGE, chains::write, chains::check),
        #[cfg(feature = "internal_error_chain")]
        Scenario::new(compliance::PACKAGE, compliance::write, compliance::check),
        #[cfg(feature = "internal_error_chain")]
        Scenario::new(signatures::PACKAGE, signatures::write, signatures::check),
        #[cfg(feature = "error_sites")]
        Scenario::new(sites::PACKAGE, sites::write, sites::check),
        #[cfg(feature = "internal_error_chain")]
        Scenario::new(types::PACKAGE, types::write, types::check),
    ])
}

//! Native error-apply evidence agrees with the compatibility loader for every
//! specimen package, sharing one semantic workspace load.
#![cfg(feature = "internal_error_chain")]
#[path = "../support/error_apply_projection.rs"]
mod apply_projection;
#[path = "../support/source_projection.rs"]
mod projection;
#[path = "../support/shared_workspace.rs"]
mod shared;

mod conversion_sites;
mod expressions;
mod getter_uses;
mod getters;
mod initializers;
mod lifetimes;
mod local_lifetime_consumption;
mod local_lifetimes;
mod safety;
mod uses;

use shared::Scenario;

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn native_error_apply_evidence_matches_the_compatibility_loader() -> miette::Result<()> {
    cordial::init_tracing();
    shared::run(&[
        Scenario::new(
            conversion_sites::PACKAGE,
            conversion_sites::write,
            conversion_sites::check,
        ),
        Scenario::new(expressions::PACKAGE, expressions::write, expressions::check),
        Scenario::new(getter_uses::PACKAGE, getter_uses::write, getter_uses::check),
        Scenario::new(getters::PACKAGE, getters::write, getters::check),
        Scenario::new(
            initializers::PACKAGE,
            initializers::write,
            initializers::check,
        ),
        Scenario::new(lifetimes::PACKAGE, lifetimes::write, lifetimes::check),
        Scenario::new(
            local_lifetime_consumption::BORROWED,
            local_lifetime_consumption::write_borrowed,
            local_lifetime_consumption::check_borrowed,
        ),
        Scenario::new(
            local_lifetime_consumption::OWNED,
            local_lifetime_consumption::write_owned,
            local_lifetime_consumption::check_owned,
        ),
        Scenario::new(
            local_lifetimes::PACKAGE,
            local_lifetimes::write,
            local_lifetimes::check,
        ),
        Scenario::new(safety::PACKAGE, safety::write, safety::check),
        Scenario::new(uses::PACKAGE, uses::write, uses::check),
    ])
}

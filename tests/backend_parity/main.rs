//! Native policy, observability and dialect facts match the compatibility
//! loader, including virtual revisions and source-free replay, sharing one
//! semantic workspace load.
#![cfg(any(
    feature = "antipatterns",
    feature = "cli_layout",
    feature = "derives",
    feature = "panics",
    feature = "tracing",
    feature = "verus_ir",
    feature = "visibility"
))]
#[cfg(feature = "tracing")]
#[path = "../support/observability_projection.rs"]
mod projection;
#[cfg(any(feature = "antipatterns", feature = "panics", feature = "verus_ir"))]
#[path = "../support/semantic_metrics.rs"]
mod semantic_metrics;
#[path = "../support/shared_workspace.rs"]
mod shared;

#[cfg(feature = "antipatterns")]
mod antipatterns;
#[cfg(feature = "cli_layout")]
mod cli_layout;
#[cfg(feature = "derives")]
mod derive;
#[cfg(feature = "tracing")]
mod observability;
#[cfg(feature = "panics")]
mod panics;
#[cfg(feature = "verus_ir")]
mod verus;
#[cfg(feature = "visibility")]
mod visibility;

use shared::Scenario;

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn native_facts_match_the_compatibility_loader() -> miette::Result<()> {
    cordial::init_tracing();
    shared::run(&[
        #[cfg(feature = "antipatterns")]
        Scenario::new(
            antipatterns::KANI,
            antipatterns::write,
            antipatterns::check_kani,
        ),
        #[cfg(feature = "antipatterns")]
        Scenario::new(
            antipatterns::CREUSOT,
            antipatterns::write,
            antipatterns::check_creusot,
        ),
        #[cfg(feature = "antipatterns")]
        Scenario::new(
            antipatterns::VERUS,
            antipatterns::write,
            antipatterns::check_verus,
        ),
        #[cfg(feature = "cli_layout")]
        Scenario::new(cli_layout::PACKAGE, cli_layout::write, cli_layout::check),
        #[cfg(feature = "derives")]
        Scenario::new(derive::PACKAGE, derive::write, derive::check),
        #[cfg(feature = "tracing")]
        Scenario::new(
            observability::PACKAGE,
            observability::write,
            observability::check,
        ),
        #[cfg(feature = "panics")]
        Scenario::new(panics::PACKAGE, panics::write, panics::check),
        #[cfg(feature = "verus_ir")]
        Scenario::new(verus::PACKAGE, verus::write, verus::check),
        #[cfg(feature = "visibility")]
        Scenario::new(visibility::PACKAGE, visibility::write, visibility::check),
    ])
}

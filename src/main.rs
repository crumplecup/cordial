//! Binary entry: parse args, dispatch through the library, surface with miette.

use clap::Parser;
use cordial::{Cli, init_cli_tracing};
use miette::IntoDiagnostic;

use tracing::instrument;

#[cfg(all(feature = "alloc_metrics", not(feature = "hotpath-alloc")))]
mod counting_allocator;

#[cfg(all(feature = "alloc_metrics", not(feature = "hotpath-alloc")))]
#[global_allocator]
static ALLOCATOR: counting_allocator::Counting = counting_allocator::Counting;

#[instrument(level = "info", err(level = "warn"))]
#[hotpath::measure]
fn main() -> miette::Result<()> {
    #[cfg(all(feature = "alloc_metrics", not(feature = "hotpath-alloc")))]
    salsa::set_allocation_probe(counting_allocator::net_bytes);
    #[cfg(all(feature = "alloc_metrics", not(feature = "hotpath-alloc")))]
    salsa::set_allocation_tags(salsa::AllocationTags {
        set_tag: counting_allocator::set_tag,
        held_by_tag: counting_allocator::held_by_tag,
        held_in_all: counting_allocator::held_in_all,
        peak_held: counting_allocator::peak_held,
        freed_pairs: counting_allocator::freed_pairs,
        freed_by_age: counting_allocator::freed_by_age,
    });
    #[cfg(all(feature = "alloc_metrics", not(feature = "hotpath-alloc")))]
    cordial::start_heap_sampling();
    init_cli_tracing();
    Cli::parse().act().into_diagnostic()
}

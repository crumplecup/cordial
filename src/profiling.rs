//! The program-wide pieces of `hotpath` profiling: what every binary that links
//! this library shares, so a profile covers the CLI and each test the same way.
//!
//! Timing needs only `#[hotpath::measure]` on the functions. Allocation needs one
//! counting allocator for the whole program, declared here, so a binary does not
//! declare its own (a second would not link). The guard that writes a profile
//! belongs here too once there is one place that builds it.

/// The program's one allocator when allocation profiling is on: it counts the
/// bytes and the allocations of every function hotpath measures, and of every
/// thread. Without the feature the program keeps the system allocator.
#[cfg(feature = "hotpath-alloc")]
#[global_allocator]
static ALLOCATOR: hotpath::CountingAllocator = hotpath::CountingAllocator::with(std::alloc::System);

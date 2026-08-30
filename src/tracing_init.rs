//! Install the process tracing subscriber.

use tracing_subscriber::EnvFilter;

/// Read `RUST_LOG` (default `info`) and install a fmt subscriber.
///
/// Safe to call more than once (`try_init` ignores an already-set subscriber).
/// `main` and each `#[test]` under `tests/` should call this.
#[tracing::instrument(level = "debug")]
pub fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}

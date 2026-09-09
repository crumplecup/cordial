//! Install the process tracing subscriber.

use tracing_subscriber::EnvFilter;

/// Read `RUST_LOG` (default `info`) and install a fmt subscriber.
///
/// Safe to call more than once (`try_init` ignores an already-set subscriber).
/// `main` and each `#[test]` under `tests/` should call this.
#[tracing::instrument(level = "debug")]
pub fn init_tracing() {
    init_tracing_with_default("info");
}

/// Install the process tracing subscriber for the CLI.
///
/// The CLI uses progress indicators for routine responsiveness, so tracing
/// defaults to warnings unless `RUST_LOG` asks for more detail.
#[tracing::instrument(level = "debug")]
pub fn init_cli_tracing() {
    init_tracing_with_default("warn");
}

#[tracing::instrument(level = "debug")]
fn init_tracing_with_default(default_filter: &str) {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_filter));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}

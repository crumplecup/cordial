//! Install the process tracing subscriber.

use tracing_subscriber::EnvFilter;

/// Cordial's own events at `info`; dependencies only at `warn`.
const TEST_DEFAULT_FILTER: &str = "warn,cordial=info";

/// Read `RUST_LOG` and install a fmt subscriber.
///
/// The default shows cordial's own events at `info` and dependencies at `warn`
/// (`warn,cordial=info`). Dependencies such as `salsa` log `info` for every
/// query execution, which drowns out cordial's events without telling a test
/// author anything. Nothing is silenced: dependency warnings and errors still
/// appear, and `RUST_LOG=salsa=info` (or similar) brings their detail back.
///
/// Safe to call more than once (`try_init` ignores an already-set subscriber).
/// `main` and each `#[test]` under `tests/` should call this.
#[tracing::instrument(level = "debug")]
pub fn init_tracing() {
    init_tracing_with_default(TEST_DEFAULT_FILTER);
}

/// Install the process tracing subscriber for the CLI.
///
/// The CLI uses progress indicators for routine responsiveness, so tracing
/// defaults to warnings unless `RUST_LOG` asks for more detail.
#[tracing::instrument(level = "debug")]
pub fn init_cli_tracing() {
    init_tracing_with_default("warn");
}

/// A `fmt` layer for tests that install their own thread-local subscriber.
///
/// A test that attaches a recorder layer with `tracing::subscriber::set_default`
/// replaces the global subscriber from [`init_tracing`], so its output would
/// vanish. Adding this layer keeps the same output and `RUST_LOG` handling.
/// The filter applies to this layer alone, so recorder layers still see every
/// event and span they assert on.
#[tracing::instrument(level = "debug")]
pub fn tracing_fmt_layer<S>() -> impl tracing_subscriber::Layer<S>
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
    use tracing_subscriber::Layer;
    tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_filter(env_filter(TEST_DEFAULT_FILTER))
}

#[tracing::instrument(level = "debug")]
fn env_filter(default_filter: &str) -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_filter))
}

#[tracing::instrument(level = "debug")]
fn init_tracing_with_default(default_filter: &str) {
    // The install site reads `RUST_LOG` itself; the subscriber policy checks
    // for `try_from_default_env` + `unwrap_or*` in the function that installs.
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_filter));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}

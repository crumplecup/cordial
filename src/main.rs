//! Binary entry: parse args, dispatch through the library, surface with miette.

use clap::Parser;
use cordial::{Cli, init_cli_tracing};
use miette::IntoDiagnostic;

use tracing::instrument;
#[instrument(level = "info", err(level = "warn"))]
fn main() -> miette::Result<()> {
    init_cli_tracing();
    let args: Vec<_> = std::env::args_os().collect();
    if let Some(code) = cordial::host::forward(&args) {
        std::process::exit(code);
    }
    Cli::parse().act().into_diagnostic()
}

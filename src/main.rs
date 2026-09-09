//! Binary entry: parse args, dispatch through the library, surface with miette.

use clap::Parser;
use cordial::{Cli, init_cli_tracing};
use miette::IntoDiagnostic;

use tracing::instrument;
#[instrument(level = "info", err(level = "warn"))]
fn main() -> miette::Result<()> {
    init_cli_tracing();
    Cli::parse().act().into_diagnostic()
}

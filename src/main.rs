//! Binary entry: parse args, dispatch through the library, surface with miette.

use clap::Parser;
use cordial::{Cli, init_cli_tracing};
use miette::IntoDiagnostic;

use tracing::instrument;

fn main() -> std::process::ExitCode {
    let profile = cordial::profiling::ProcessProfile::start();
    if let Err(error) = &profile {
        eprintln!("cordial: could not start Hotpath capture: {error}");
    }
    let code = match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("{error:?}");
            1
        }
    };
    if let Ok(profile) = profile {
        profile.finish(if code == 0 {
            cordial::profiling::FinishReason::Normal
        } else {
            cordial::profiling::FinishReason::Error
        });
    }
    std::process::ExitCode::from(u8::try_from(code).unwrap_or(1))
}

#[instrument(level = "info", err(level = "warn"))]
#[hotpath::measure]
fn run() -> miette::Result<i32> {
    init_cli_tracing();
    match Cli::try_parse() {
        Ok(cli) => cli.act_with_exit_code().into_diagnostic(),
        Err(error) => {
            let code = error.exit_code();
            error.print().into_diagnostic()?;
            Ok(code)
        }
    }
}

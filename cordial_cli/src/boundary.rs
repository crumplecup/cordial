//! Binary-only error reporting with miette.
//!
//! Linked from `main.rs` only — keeps miette out of the `cordial_cli` library API.

use std::process;

use cordial_cli::CliError;
use miette::{Diagnostic, Report};

use tracing::instrument;
/// Run the CLI and render failures with miette.
#[instrument(level = "info", err(level = "warn"))]
pub fn run() -> Result<(), Report> {
    install_hook();
    cordial_cli::run().map_err(|err| Report::from(BinaryError(err)))
}

/// Print a diagnostic report to stderr and exit with status 1.
#[instrument(level = "debug", skip(report))]
pub fn exit_on_error(report: Report) {
    eprintln!("{report:?}");
    process::exit(1);
}

fn install_hook() {
    let _ = miette::set_hook(Box::new(|_| {
        Box::new(
            miette::MietteHandlerOpts::new()
                .terminal_links(true)
                .unicode(true)
                .build(),
        )
    }));
}

/// Local wrapper so `Diagnostic` can be implemented without touching library types.
#[derive(Debug)]
struct BinaryError(CliError);

impl std::fmt::Display for BinaryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

impl std::error::Error for BinaryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

impl Diagnostic for BinaryError {
    fn code<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        Some(Box::new(format!(
            "cordial_cli::{}",
            error_kind_code(&self.0)
        )))
    }

    fn help<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        Some(Box::new(
            "Library code should return `CordialError`; this binary surfaces failures with miette.",
        ))
    }
}

fn error_kind_code(err: &CliError) -> &'static str {
    match err {
        CliError::Io(_) => "Io",
        CliError::Cordial(_) => "Cordial",
        CliError::NotFound { .. } => "NotFound",
        CliError::NoExceptions { .. } => "NoExceptions",
        CliError::NoCachedIr { .. } => "NoCachedIr",
        CliError::CoverageFeatureDisabled => "CoverageFeatureDisabled",
        CliError::Prefix(_) => "Prefix",
    }
}

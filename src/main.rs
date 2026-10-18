//! Binary entry: parse args, dispatch through the library, surface with miette.

use clap::Parser;
use cordial::{Cli, init_cli_tracing};
use miette::IntoDiagnostic;

use tracing::instrument;
#[instrument(level = "info", err(level = "warn"))]
fn main() -> miette::Result<()> {
    init_cli_tracing();
    // Keep CLI JSON stdout clean when profiling is enabled. HOTPATH_OUTPUT_PATH
    // can override this path for repeatable profiling runs.
    #[cfg(feature = "hotpath")]
    let _hotpath = {
        let path = std::env::var_os("HOTPATH_OUTPUT_PATH").map_or_else(
            || std::env::temp_dir().join(format!("cordial-hotpath-{}.json", std::process::id())),
            std::path::PathBuf::from,
        );
        tracing::info!(path = %path.display(), "hotpath report");
        hotpath::HotpathGuardBuilder::new("cordial")
            .format(hotpath::Format::JsonPretty)
            .output_path(path)
            .functions_limit(0)
            .build()
    };
    Cli::parse().act().into_diagnostic()
}

//! The `cordial` binary as a command, and paths as the UTF-8 strings its arguments need.

use std::process::Command;

pub fn cordial_command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_cordial"));
    command.env("RUST_LOG", "info");
    command
}

pub fn utf8_path(path: &std::path::Path) -> miette::Result<&str> {
    path.to_str()
        .ok_or_else(|| miette::miette!("path is not UTF-8: {}", path.display()))
}

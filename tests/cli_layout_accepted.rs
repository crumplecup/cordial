//! Layouts that follow the rules and raise nothing.

use cordial::CliLayoutId;
use miette::WrapErr;

#[path = "support/cli_layout_fixture.rs"]
mod cli_layout_fixture;

use cli_layout_fixture::{cli_layout_records, write_cli_crate};
#[test]
fn well_formed_cli_layout_is_not_a_violation() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_cli_crate(
        r#"
use clap::{Parser, Subcommand};

#[derive(Parser)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Run,
}

impl Cli {
    pub fn act(self) -> Result<(), std::io::Error> {
        self.command.act()
    }
}

impl Commands {
    pub fn act(self) -> Result<(), std::io::Error> {
        Ok(())
    }
}
"#,
        r#"
fn main() -> Result<(), std::io::Error> {
    Cli::parse().act()
}
"#,
    )?;
    let findings = cli_layout_records(fixture.path(), "fixture").wrap_err("scan")?;
    assert!(
        !findings.iter().any(|finding| {
            matches!(
                finding.rule_id(),
                CliLayoutId::Island001 | CliLayoutId::Act001 | CliLayoutId::Main001
            )
        }),
        "library Parser + act and thin main must pass: {:?}",
        findings
    );
    Ok(())
}

#[test]
fn optional_nested_command_hand_off_through_some_is_not_a_violation() -> miette::Result<()> {
    // `#[command(subcommand)] command: Option<Commands>` is the standard
    // clap idiom for "no subcommand given -> a default action", not just
    // a stylistic variant of the required-subcommand shape covered by
    // `well_formed_cli_layout_is_not_a_violation` above. `Cli::act`
    // hands off through `match self.command { Some(command) =>
    // command.act(), None => .. }` -- the `Some(command)` pattern must
    // still be recognized as binding `command: Commands`, the same
    // nested clap type already declared on the `Option<Commands>` field,
    // not silently lost because the pattern's own path ("Some") isn't
    // one of `Commands`'s own variants.
    cordial::init_tracing();
    let fixture = write_cli_crate(
        r#"
use clap::{Parser, Subcommand};

#[derive(Parser)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    Run,
}

impl Cli {
    pub fn act(self) -> Result<(), std::io::Error> {
        match self.command {
            Some(command) => command.act(),
            None => Ok(()),
        }
    }
}

impl Commands {
    pub fn act(self) -> Result<(), std::io::Error> {
        Ok(())
    }
}
"#,
        r#"
fn main() -> Result<(), std::io::Error> {
    Cli::parse().act()
}
"#,
    )?;
    let findings = cli_layout_records(fixture.path(), "fixture").wrap_err("scan")?;
    assert!(
        !findings.iter().any(|finding| {
            matches!(
                finding.rule_id(),
                CliLayoutId::Island001 | CliLayoutId::Act001 | CliLayoutId::Main001
            )
        }),
        "Some(command) => command.act() must be recognized as handing off to Commands: {:?}",
        findings
    );
    Ok(())
}

#[test]
fn tracing_helper_call_in_main_is_not_a_violation() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_cli_crate(
        r#"
use clap::{Parser, Subcommand};

#[derive(Parser)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Run,
}

impl Cli {
    pub fn act(self) -> Result<(), std::io::Error> {
        self.command.act()
    }
}

impl Commands {
    pub fn act(self) -> Result<(), std::io::Error> {
        Ok(())
    }
}

pub fn init_tracing() {}
"#,
        r#"
fn main() -> Result<(), std::io::Error> {
    init_tracing();
    Cli::parse().act()
}
"#,
    )?;
    let findings = cli_layout_records(fixture.path(), "fixture").wrap_err("scan")?;
    assert!(
        !findings.iter().any(|finding| {
            matches!(
                finding.rule_id(),
                CliLayoutId::Island001 | CliLayoutId::Act001 | CliLayoutId::Main001
            )
        }),
        "main may call the library tracing helper once: {:?}",
        findings
    );
    Ok(())
}

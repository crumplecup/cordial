//! The hand-off from `main` to `act` and on to nested commands: where it is broken.

use cordial::CliLayoutId;
use miette::WrapErr;

#[path = "support/cli_layout_fixture.rs"]
mod cli_layout_fixture;

use cli_layout_fixture::{cli_layout_records, write_cli_crate};
#[test]
fn parser_without_act_is_a_violation() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_cli_crate(
        r#"
use clap::Parser;

#[derive(Parser)]
pub struct Cli {
    name: String,
}
"#,
        "fn main() {}\n",
    )?;
    let findings = cli_layout_records(fixture.path(), "fixture").wrap_err("scan")?;
    assert!(
        findings.iter().any(|finding| {
            finding.rule_id() == CliLayoutId::Act001 && finding.context().contains("Cli")
        }),
        "Parser without act must be CLI-ACT: {:?}",
        findings
    );
    Ok(())
}

#[test]
fn match_in_main_is_a_violation() -> miette::Result<()> {
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
fn main() {
    let _ = Cli::parse().act();
    match 1 {
        1 => {}
        _ => {}
    }
}
"#,
    )?;
    let findings = cli_layout_records(fixture.path(), "fixture").wrap_err("scan")?;
    assert!(
        findings
            .iter()
            .any(|finding| finding.rule_id() == CliLayoutId::Main001),
        "match in main must be CLI-MAIN: {:?}",
        findings
    );
    Ok(())
}

#[test]
fn subcommand_without_act_is_a_violation() -> miette::Result<()> {
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
"#,
        r#"
fn main() -> Result<(), std::io::Error> {
    Cli::parse().act()
}
"#,
    )?;
    let findings = cli_layout_records(fixture.path(), "fixture").wrap_err("scan")?;
    assert!(
        findings.iter().any(|finding| {
            finding.rule_id() == CliLayoutId::Act001 && finding.context().contains("Commands")
        }),
        "Subcommand without act must be CLI-ACT: {:?}",
        findings
    );
    Ok(())
}

#[test]
fn parser_act_must_hand_off_to_nested_command() -> miette::Result<()> {
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
        Ok(())
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
        findings.iter().any(|finding| {
            finding.rule_id() == CliLayoutId::Act001 && finding.snippet().contains("nested clap")
        }),
        "Parser::act that does not call Commands::act must be CLI-ACT: {:?}",
        findings
    );
    Ok(())
}

#[test]
fn free_function_taking_cli_is_a_violation() -> miette::Result<()> {
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

fn handle(cli: Cli) -> Result<(), std::io::Error> {
    cli.act()
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
        findings.iter().any(|finding| {
            finding.rule_id() == CliLayoutId::Act001 && finding.snippet().contains("free function")
        }),
        "free fn taking Cli must be CLI-ACT: {:?}",
        findings
    );
    Ok(())
}

#[test]
fn free_function_taking_option_cli_is_a_violation() -> miette::Result<()> {
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

fn handle(cli: Option<Cli>) -> Result<(), std::io::Error> {
    match cli {
        Some(cli) => cli.act(),
        None => Ok(()),
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
        findings.iter().any(|finding| {
            finding.rule_id() == CliLayoutId::Act001 && finding.snippet().contains("free function")
        }),
        "free fn taking Option<Cli> must be CLI-ACT: {:?}",
        findings
    );
    Ok(())
}

#[test]
fn act_must_hand_off_every_nested_clap_type() -> miette::Result<()> {
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
    Exceptions {
        #[command(subcommand)]
        command: ExceptionCommands,
    },
    Export {
        #[command(subcommand)]
        command: ExportCommands,
    },
}

#[derive(Subcommand)]
pub enum ExceptionCommands {
    List,
}

#[derive(Subcommand)]
pub enum ExportCommands {
    Surreal,
}

impl Cli {
    pub fn act(self) -> Result<(), std::io::Error> {
        self.command.act()
    }
}

impl Commands {
    pub fn act(self) -> Result<(), std::io::Error> {
        match self {
            Self::Exceptions { command } => command.act(),
            Self::Export { .. } => Ok(()),
        }
    }
}

impl ExceptionCommands {
    pub fn act(self) -> Result<(), std::io::Error> {
        Ok(())
    }
}

impl ExportCommands {
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
        findings.iter().any(|finding| {
            finding.rule_id() == CliLayoutId::Act001
                && finding.snippet().contains("ExportCommands")
                && !finding.snippet().contains("ExceptionCommands")
        }),
        "skipping one nested clap type must be CLI-ACT: {:?}",
        findings
    );
    Ok(())
}

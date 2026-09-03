use miette::{IntoDiagnostic, WrapErr};
use std::fs;
use std::path::Path;

use cordial::{
    CLI_LAYOUT_ETIQUETTE, CliLayoutId, RunAll, Session, SessionBuilder, scan_crate_cli_layout,
};

fn write_cli_crate(lib: &str, main: &str) -> miette::Result<tempfile::TempDir> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(fixture.path().join("src/lib.rs"), lib)
        .into_diagnostic()
        .wrap_err("write lib")?;
    fs::write(fixture.path().join("src/main.rs"), main)
        .into_diagnostic()
        .wrap_err("write main")?;
    Ok(fixture)
}

#[test]
fn clap_types_only_in_main_are_an_island() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_cli_crate(
        "pub struct Lib;\n",
        r#"
use clap::Parser;

#[derive(Parser)]
struct Cli {
    name: String,
}

fn main() {}
"#,
    )?;
    let findings = scan_crate_cli_layout(fixture.path(), "fixture")
        .into_diagnostic()
        .wrap_err("scan")?;
    assert!(
        findings.iter().any(|finding| {
            finding.rule_id() == CliLayoutId::Island001 && finding.context().contains("Cli")
        }),
        "Parser in main.rs must be CLI-ISLAND: {:?}",
        findings
    );
    Ok(())
}

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
    let findings = scan_crate_cli_layout(fixture.path(), "fixture")
        .into_diagnostic()
        .wrap_err("scan")?;
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
    let findings = scan_crate_cli_layout(fixture.path(), "fixture")
        .into_diagnostic()
        .wrap_err("scan")?;
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
fn bin_only_error_type_is_an_island() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_cli_crate(
        "pub struct Lib;\n",
        r#"
struct BinaryError;

impl std::error::Error for BinaryError {}
impl std::fmt::Display for BinaryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "bin")
    }
}

fn main() {}
"#,
    )?;
    let findings = scan_crate_cli_layout(fixture.path(), "fixture")
        .into_diagnostic()
        .wrap_err("scan")?;
    assert!(
        findings.iter().any(|finding| {
            finding.rule_id() == CliLayoutId::Island001 && finding.context().contains("BinaryError")
        }),
        "Error type in main.rs must be CLI-ISLAND: {:?}",
        findings
    );
    Ok(())
}

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
    let findings = scan_crate_cli_layout(fixture.path(), "fixture")
        .into_diagnostic()
        .wrap_err("scan")?;
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
    let findings = scan_crate_cli_layout(fixture.path(), "fixture")
        .into_diagnostic()
        .wrap_err("scan")?;
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
    let findings = scan_crate_cli_layout(fixture.path(), "fixture")
        .into_diagnostic()
        .wrap_err("scan")?;
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
    let findings = scan_crate_cli_layout(fixture.path(), "fixture")
        .into_diagnostic()
        .wrap_err("scan")?;
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
    let findings = scan_crate_cli_layout(fixture.path(), "fixture")
        .into_diagnostic()
        .wrap_err("scan")?;
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
    let findings = scan_crate_cli_layout(fixture.path(), "fixture")
        .into_diagnostic()
        .wrap_err("scan")?;
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
    let findings = scan_crate_cli_layout(fixture.path(), "fixture")
        .into_diagnostic()
        .wrap_err("scan")?;
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
    let findings = scan_crate_cli_layout(fixture.path(), "fixture")
        .into_diagnostic()
        .wrap_err("scan")?;
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

#[test]
fn dogfood_cordial_follows_cli_layout() -> miette::Result<()> {
    cordial::init_tracing();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let findings = scan_crate_cli_layout(root, "cordial")
        .into_diagnostic()
        .wrap_err("scan cordial")?;
    assert!(
        findings.is_empty(),
        "cordial should follow CLI layout: {findings:?}"
    );
    Ok(())
}

#[test]
fn session_writes_cli_layout_artifacts() -> miette::Result<()> {
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
    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&CLI_LAYOUT_ETIQUETTE)
        .build();
    let outcome = session.run(&RunAll).into_diagnostic().wrap_err("run")?;
    assert!(
        outcome
            .findings()
            .any(|finding| finding.rule().id() == "CLI-ACT-001"),
        "session should surface CLI-ACT"
    );
    let findings_dir = store.path().join("findings");
    assert!(findings_dir.join("cli-layout.csv").is_file());
    assert!(findings_dir.join("cli-layout.checklist.md").is_file());
    assert!(findings_dir.join("cli-layout-summary.md").is_file());
    Ok(())
}

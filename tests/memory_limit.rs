//! A run bounds its own memory: eight gibibytes unless told otherwise, and it says where it was
//! when it stops itself.

mod support {
    pub mod mini_workspace;
}

use clap::Parser;
use cordial::{
    Cli, DEFAULT_MEMORY_LIMIT, MEMORY_LIMIT_EXIT, exceeds, load_cordial_config, parse_size,
    resolve_memory_limit,
};
use miette::{IntoDiagnostic, Result};

#[test]
fn sizes_are_binary_and_zero_is_no_limit() {
    assert_eq!(parse_size("8G"), Ok(8 << 30));
    assert_eq!(parse_size("512m"), Ok(512 << 20));
    assert_eq!(parse_size("64K"), Ok(64 << 10));
    assert_eq!(parse_size("4096"), Ok(4096));
    assert_eq!(parse_size("0"), Ok(0));
    assert!(parse_size("lots").is_err());
    assert!(parse_size("").is_err());
    assert!(
        parse_size("99999999999T").is_err(),
        "an overflow is refused"
    );
}

#[test]
fn a_process_is_over_a_limit_only_when_it_has_one() {
    let limit = parse_size("1G").unwrap_or_default();
    assert!(!exceeds(1024 * 1024, limit), "1 GiB of 1 GiB is not over");
    assert!(exceeds(1024 * 1024 + 1, limit), "a kibibyte more is");
    assert!(!exceeds(u64::MAX, 0), "zero is no limit");
}

#[test]
fn the_flag_beats_the_config_and_the_config_beats_the_default() -> Result<()> {
    assert_eq!(parse_size(DEFAULT_MEMORY_LIMIT), Ok(8 << 30));
    let eight = 8 << 30;

    // Nothing says: eight gibibytes.
    let none = Cli::try_parse_from(["cordial", "open"]).into_diagnostic()?;
    assert_eq!(none.memory_limit, None);
    assert_eq!(resolve_memory_limit(none.memory_limit, None), eight);

    // The config says.
    assert_eq!(resolve_memory_limit(None, Some("12G")), 12 << 30);
    assert_eq!(resolve_memory_limit(None, Some("0")), 0);

    // The flag says, before or after the subcommand, and beats the config.
    let before =
        Cli::try_parse_from(["cordial", "--memory-limit", "16G", "open"]).into_diagnostic()?;
    assert_eq!(
        resolve_memory_limit(before.memory_limit, Some("12G")),
        16 << 30
    );
    let after =
        Cli::try_parse_from(["cordial", "open", "--memory-limit", "0"]).into_diagnostic()?;
    assert_eq!(resolve_memory_limit(after.memory_limit, Some("12G")), 0);

    // A setting that is not a size is refused with the default, not obeyed or fatal.
    assert_eq!(resolve_memory_limit(None, Some("lots")), eight);
    Ok(())
}

#[test]
fn cordial_toml_can_set_the_limit() -> Result<()> {
    let project = tempfile::tempdir().into_diagnostic()?;
    let home = tempfile::tempdir().into_diagnostic()?;
    assert_eq!(
        load_cordial_config(project.path(), home.path()).memory_limit(),
        &None,
        "unset by default"
    );
    std::fs::write(
        project.path().join("cordial.toml"),
        "memory_limit = \"16G\"\n",
    )
    .into_diagnostic()?;
    let config = load_cordial_config(project.path(), home.path());
    assert_eq!(config.memory_limit().as_deref(), Some("16G"));
    assert_eq!(
        resolve_memory_limit(None, config.memory_limit().as_deref()),
        16 << 30
    );
    Ok(())
}

#[test]
fn a_run_over_its_limit_stops_itself_and_says_where_and_how_to_raise_it() -> Result<()> {
    cordial::init_tracing();
    let scratch = tempfile::tempdir().into_diagnostic()?;
    let project = scratch.path().join("mini");
    let store = scratch.path().join("store");
    support::mini_workspace::write(&project)?;

    // Loading a workspace takes far more than this.
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_cordial"))
        .arg("--project")
        .arg(&project)
        .arg("--store-home")
        .arg(&store)
        .args(["--memory-limit", "16M", "open"])
        .output()
        .into_diagnostic()?;
    let text = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(MEMORY_LIMIT_EXIT),
        "stopped by its own limit: {text}"
    );
    assert!(text.contains("cordial stopped itself"), "{text}");
    assert!(
        text.contains("--memory-limit"),
        "it says how to raise it: {text}"
    );
    assert!(text.contains("during \""), "it says where it was: {text}");
    Ok(())
}

#[test]
fn the_limit_in_cordial_toml_stops_a_run_too() -> Result<()> {
    cordial::init_tracing();
    let scratch = tempfile::tempdir().into_diagnostic()?;
    let project = scratch.path().join("mini");
    let store = scratch.path().join("store");
    support::mini_workspace::write(&project)?;
    std::fs::write(project.join("cordial.toml"), "memory_limit = \"16M\"\n").into_diagnostic()?;

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_cordial"))
        .arg("--project")
        .arg(&project)
        .arg("--store-home")
        .arg(&store)
        .arg("open")
        .output()
        .into_diagnostic()?;
    let text = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(MEMORY_LIMIT_EXIT), "{text}");
    assert!(text.contains("cordial stopped itself"), "{text}");
    Ok(())
}

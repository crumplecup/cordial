//! An error-path event is `tracing`'s own `warn!` or `error!`, wherever in the
//! body it is, and not a macro that merely has that name.

use cordial::{RunAll, Session, SessionBuilder, TRACING_ETIQUETTE};
use miette::{IntoDiagnostic, WrapErr};
use std::fs;

const LIBRARY: &str = include_str!("fixtures/tracing_events/lib.rs.txt");

/// The rule ids reported for `function`, from the instrument findings CSV.
fn rules_for(csv: &str, function: &str) -> Vec<String> {
    csv.lines()
        .filter(|line| line.contains(function))
        .filter_map(|line| line.split(',').find(|field| field.starts_with("TRACING-")))
        .map(str::to_owned)
        .collect()
}

fn findings() -> miette::Result<String> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src")).into_diagnostic()?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[dependencies]\ntracing = '0.1'\n[workspace]\n",
    )
    .into_diagnostic()?;
    fs::write(fixture.path().join("src/lib.rs"), LIBRARY).into_diagnostic()?;
    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&*TRACING_ETIQUETTE)
        .build();
    session.run(&RunAll).into_diagnostic().wrap_err("run")?;
    fs::read_to_string(store.path().join("findings/tracing-instrument.csv"))
        .into_diagnostic()
        .wrap_err("csv")
}

#[test]
fn a_local_macro_named_warn_is_not_an_error_event() -> miette::Result<()> {
    let csv = findings()?;
    let rules = rules_for(&csv, "local_lookalike");
    assert!(
        rules.contains(&"TRACING-ERROR-PATH-SILENT".to_owned()),
        "{csv}"
    );
    Ok(())
}

#[test]
fn tracing_own_event_counts_wherever_it_is() -> miette::Result<()> {
    let csv = findings()?;
    for function in ["real_event", "event_in_closure"] {
        let rules = rules_for(&csv, function);
        assert!(
            !rules.contains(&"TRACING-ERROR-PATH-SILENT".to_owned()),
            "{function}: {csv}"
        );
        assert!(
            rules.contains(&"TRACING-ERR-MISSING".to_owned()),
            "{function}: {csv}"
        );
    }
    Ok(())
}

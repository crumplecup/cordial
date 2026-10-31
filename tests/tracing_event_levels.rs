//! Severity requirements consume typed IR and preserve deliberate annotations.

use cordial::{RunAll, Session, SessionBuilder, TRACING_ETIQUETTE};
use miette::{IntoDiagnostic, Result};
use std::fs;

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn error_severity_is_not_boolean_presence() -> Result<()> {
    cordial::init_cli_tracing();
    let fixture = tempfile::tempdir().into_diagnostic()?;
    let store = tempfile::tempdir().into_diagnostic()?;
    fs::create_dir(fixture.path().join("src")).into_diagnostic()?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = \"severity\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[workspace]\n",
    )
    .into_diagnostic()?;
    let mut source = String::new();
    for (name, err) in [
        ("weak_trace", "err(level = \"trace\")"),
        ("weak_debug", "err(Debug, level = \"debug\")"),
        ("weak_info", "err(level = \"info\")"),
        ("compliant_warn", "err(level = \"warn\")"),
        ("compliant_error", "err(level = \"error\")"),
        ("compliant_default", "err"),
        ("compliant_debug_format", "err(Debug)"),
        ("unknown", "err(level = EVENT_LEVEL)"),
        ("shadowed_constant", "err(level = fake::Level::WARN)"),
        ("unknown_span", "err"),
        ("ambiguous", "err(level = \"warn\", level = \"debug\")"),
    ] {
        let span = if name == "unknown_span" {
            "SPAN_LEVEL"
        } else {
            "\"trace\""
        };
        source.push_str(&format!("#[tracing::instrument(level = {span}, skip_all, {err})]\npub fn {name}() -> Result<(), std::io::Error> {{ Ok(()) }}\n"));
    }
    // Literal contents must not invent top-level arguments or hide real ones.
    source.push_str("#[tracing::instrument(level = \"trace\", fields(note = \"), err(level = \\\"warn\\\")\"), err(level = \"debug\"))]\npub fn quoted_argument() -> Result<(), std::io::Error> { Ok(()) }\n");
    fs::write(fixture.path().join("src/lib.rs"), source).into_diagnostic()?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&*TRACING_ETIQUETTE)
        .build();
    session.run(&RunAll).into_diagnostic()?;
    let csv = fs::read_to_string(store.path().join("findings/tracing-instrument.csv"))
        .into_diagnostic()?;
    for name in ["weak_trace", "weak_debug", "weak_info", "quoted_argument"] {
        let rows: Vec<_> = csv
            .lines()
            .filter(|line| line.contains(&format!(",{name},")))
            .collect();
        assert_eq!(
            rows.len(),
            1,
            "one actionable severity repair for {name}: {csv}"
        );
        assert!(
            rows[0].contains("TRACING-ERR-LEVEL-MISMATCH"),
            "{}",
            rows[0]
        );
    }
    for name in [
        "compliant_warn",
        "compliant_error",
        "compliant_default",
        "compliant_debug_format",
    ] {
        assert!(
            !csv.contains(&format!(",{name},")),
            "preserve deliberate sufficient reporting: {csv}"
        );
    }
    for name in ["unknown", "unknown_span", "shadowed_constant", "ambiguous"] {
        assert!(
            csv.lines().any(|line| line.contains(&format!(",{name},"))
                && line.contains("TRACING-INSTRUMENT-UNRESOLVED")),
            "unknown evidence must stay visible for {name}: {csv}"
        );
    }
    Ok(())
}

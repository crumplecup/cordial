use miette::{IntoDiagnostic, WrapErr};
use std::fs;
use std::path::Path;

use cordial::{
    ERROR_CHAIN_ETIQUETTE, ErrorChainProbeId, RunAll, Session, SessionBuilder, probe_counts,
    scan_error_chain_rust_source,
};

const PRESERVED_FIXTURE: &str = r#"
use std::io;

#[derive(Debug)]
struct IoSource {
    source: io::Error,
}

#[derive(Debug)]
enum UmbrellaKind {
    Io(IoSource),
}

impl From<io::Error> for IoSource {
    fn from(source: io::Error) -> Self {
        Self { source }
    }
}

impl From<IoSource> for UmbrellaKind {
    fn from(value: IoSource) -> Self {
        Self::Io(value)
    }
}

impl UmbrellaKind {
    fn from_io(err: io::Error) -> Self {
        Self::Io(IoSource { source: err })
    }

    fn syn_parse(_path: String, _err: syn::Error) -> Self {
        unimplemented!()
    }
}

fn preserved_direct() -> Result<(), UmbrellaKind> {
    std::fs::read_to_string("x")?;
    Ok(())
}

fn preserved_map_err_from() -> Result<(), UmbrellaKind> {
    std::fs::read_to_string("x").map_err(IoSource::from)?;
    Ok(())
}

fn preserved_map_err_into() -> Result<(), UmbrellaKind> {
    std::fs::read_to_string("x").map_err(Into::into)?;
    Ok(())
}

fn preserved_map_err_closure() -> Result<(), UmbrellaKind> {
    std::fs::read_to_string("x").map_err(|e| UmbrellaKind::Io(IoSource { source: e }))?;
    Ok(())
}

fn preserved_map_err_forwarding_ctor() -> Result<(), UmbrellaKind> {
    std::fs::read_to_string("x").map_err(|err| UmbrellaKind::from_io(err))?;
    Ok(())
}

fn preserved_map_err_ctor_fn() -> Result<(), UmbrellaKind> {
    std::fs::read_to_string("x").map_err(UmbrellaKind::from_io)?;
    Ok(())
}

fn preserved_map_err_tail() -> Result<(), UmbrellaKind> {
    std::fs::read_to_string("x").map_err(IoSource::from)
}

fn preserved_map_err_syn_parse(path: &std::path::Path) -> Result<(), UmbrellaKind> {
    syn::parse_file("").map_err(|err| UmbrellaKind::syn_parse(path.display().to_string(), err))?;
    Ok(())
}

fn broken_stringify() -> Result<(), UmbrellaKind> {
    std::fs::read_to_string("x").map_err(|e| UmbrellaKind::Io(IoSource {
        source: io::Error::new(io::ErrorKind::Other, e.to_string()),
    }))?;
    Ok(())
}
"#;

fn scan_fixture() -> miette::Result<Vec<cordial::ErrorChainRecord>> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let file = fixture.path().join("preserved.rs");
    fs::write(&file, PRESERVED_FIXTURE)
        .into_diagnostic()
        .wrap_err("write fixture")?;
    scan_error_chain_rust_source(PRESERVED_FIXTURE, &file, fixture.path(), fixture.path())
        .into_diagnostic()
        .wrap_err("scan")
}

#[test]
fn wrapper_source_field_is_detected() -> miette::Result<()> {
    cordial::init_tracing();
    let findings = scan_fixture()?;
    assert!(findings.iter().any(|f| {
        f.rule_id == ErrorChainProbeId::WrapperSourceField001 && f.snippet.contains("IoSource")
    }));
    Ok(())
}

#[test]
fn from_bridge_is_detected() -> miette::Result<()> {
    cordial::init_tracing();
    let findings = scan_fixture()?;
    assert!(
        findings
            .iter()
            .any(|f| f.rule_id == ErrorChainProbeId::FromBridge001)
    );
    Ok(())
}

#[test]
fn preserved_question_mark_on_foreign_call() -> miette::Result<()> {
    cordial::init_tracing();
    let findings = scan_fixture()?;
    assert!(findings.iter().any(|f| {
        f.rule_id == ErrorChainProbeId::PreservedQuestionMark001
            && f.context.contains("preserved_direct")
    }));
    Ok(())
}

#[test]
fn preserved_map_err_propagation_is_detected() -> miette::Result<()> {
    cordial::init_tracing();
    let findings = scan_fixture()?;
    assert!(findings.iter().any(|f| {
        f.rule_id == ErrorChainProbeId::PreservedMapErr001
            && f.context.contains("preserved_map_err_from")
    }));
    assert!(findings.iter().any(|f| {
        f.rule_id == ErrorChainProbeId::PreservedMapErr001
            && f.context.contains("preserved_map_err_closure")
    }));
    assert!(findings.iter().any(|f| {
        f.rule_id == ErrorChainProbeId::PreservedMapErr001
            && f.context.contains("preserved_map_err_forwarding_ctor")
    }));
    assert!(findings.iter().any(|f| {
        f.rule_id == ErrorChainProbeId::PreservedMapErr001
            && f.context.contains("preserved_map_err_ctor_fn")
    }));
    assert!(findings.iter().any(|f| {
        f.rule_id == ErrorChainProbeId::PreservedMapErr001
            && f.context.contains("preserved_map_err_tail")
    }));
    assert!(findings.iter().any(|f| {
        f.rule_id == ErrorChainProbeId::PreservedMapErr001
            && f.context.contains("preserved_map_err_syn_parse")
    }));
    Ok(())
}

#[test]
fn stringifying_map_err_is_not_preserved() -> miette::Result<()> {
    cordial::init_tracing();
    let findings = scan_fixture()?;
    assert!(
        !findings
            .iter()
            .any(|f| f.context.contains("broken_stringify"))
    );
    Ok(())
}

#[test]
fn session_produces_error_chain_preserved_csv() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(fixture.path().join("src/lib.rs"), PRESERVED_FIXTURE)
        .into_diagnostic()
        .wrap_err("write fixture")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&ERROR_CHAIN_ETIQUETTE)
        .build();

    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    assert!(outcome.findings().count() >= 6);

    let findings_dir = store.path().join("findings");
    let csv = fs::read_to_string(findings_dir.join("error-chain-preserved.csv"))
        .into_diagnostic()
        .wrap_err("error-chain-preserved csv")?;
    assert!(csv.contains("ERROR-CHAIN-PRESERVED-QUESTION-MARK-001"));
    assert!(csv.contains("ERROR-CHAIN-WRAPPER-SOURCE-001"));
    assert!(csv.contains("ERROR-CHAIN-FROM-BRIDGE-001"));
    assert!(csv.contains("ERROR-CHAIN-PRESERVED-MAP-ERR-001"));

    let checklist = fs::read_to_string(findings_dir.join("error-chain-preserved.checklist.md"))
        .into_diagnostic()
        .wrap_err("checklist")?;
    assert!(checklist.contains("Error chain preserved"));
    assert!(checklist.contains("- [x]"));

    let summary = fs::read_to_string(findings_dir.join("error-chain-preserved-summary.md"))
        .into_diagnostic()
        .wrap_err("summary")?;
    assert!(summary.contains("propagation"));
    assert!(summary.contains("Infrastructure"));
    Ok(())
}

#[test]
fn probe_counts_aggregate_fixture_rules() -> miette::Result<()> {
    cordial::init_tracing();
    let records = scan_fixture()?;
    let counts = probe_counts(&records);
    assert_eq!(counts.wrapper_source, 1);
    assert_eq!(counts.from_bridge, 1);
    assert!(counts.preserved_propagation() >= 5);
    assert!(counts.infrastructure() >= 2);
    Ok(())
}

#[test]
fn scan_rust_source_accepts_relative_paths() -> miette::Result<()> {
    cordial::init_tracing();
    let findings = scan_error_chain_rust_source(
        PRESERVED_FIXTURE,
        Path::new("src/lib.rs"),
        Path::new("src"),
        Path::new("."),
    )
    .into_diagnostic()
    .wrap_err("scan")?;
    assert!(!findings.is_empty());
    Ok(())
}

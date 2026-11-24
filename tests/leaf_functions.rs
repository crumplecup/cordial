//! An integration-test binary reads off the library: its functions are recorded
//! by location with the definitions their calls resolved to, under the `test` cfg
//! that makes `#[test]` functions active, and the library graph gains no node.

use cordial::{CfgContextMode, CrateTarget, IrView, SemanticCargoOptions, SourceDatabase};
use miette::{IntoDiagnostic, Result};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tracing_subscriber::layer::SubscriberExt;

static SERIAL: Mutex<()> = Mutex::new(());

/// Counts the files and crates a context copies, which a context that flips the crates does not.
#[derive(Clone, Default)]
struct Copies(Arc<AtomicUsize>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Copies {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        struct Message(String);
        impl tracing::field::Visit for Message {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                if field.name() == "message" {
                    self.0 = format!("{value:?}");
                }
            }
        }
        let mut message = Message(String::new());
        event.record(&mut message);
        if message.0.contains("the files this context copies") {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
}

const LIBRARY: &str = include_str!("fixtures/leaf_functions/lib.rs.txt");
const SMOKE: &str = include_str!("fixtures/leaf_functions/smoke.rs.txt");
const CONJUNCTION: &str = include_str!("fixtures/leaf_functions/conjunction.rs.txt");
const GATED: &str = include_str!("fixtures/leaf_functions/gated.rs.txt");

fn a_test_function_is_recorded_with_what_it_calls(mode: CfgContextMode, copies: &Copies) -> Result<()> {
    cordial::init_tracing();
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir_all(root.path().join("src")).into_diagnostic()?;
    std::fs::create_dir_all(root.path().join("tests")).into_diagnostic()?;
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='leafy'\nversion='0.1.0'\nedition='2024'\n[dependencies]\ntracing='0.1'\n[features]\ndefault=['quick']\nquick=[]\nextra=[]\n[workspace]\n",
    )
    .into_diagnostic()?;
    std::fs::write(root.path().join("src/lib.rs"), LIBRARY).into_diagnostic()?;
    std::fs::write(root.path().join("tests/smoke.rs"), SMOKE).into_diagnostic()?;
    std::fs::write(root.path().join("tests/gated.rs"), GATED).into_diagnostic()?;
    std::fs::write(root.path().join("tests/conjunction.rs"), CONJUNCTION).into_diagnostic()?;
    let options = SemanticCargoOptions::default().with_cfg_context_mode(mode);
    let subscriber = tracing_subscriber::registry().with(copies.clone());
    let graph = tracing::subscriber::with_default(subscriber, || -> Result<_> {
        let mut database = SourceDatabase::load_with(root.path(), &options).into_diagnostic()?;
        database
            .project_target(&CrateTarget::new("leafy", root.path()))
            .into_diagnostic()
    })?;
    let leaves = graph.leaf_functions().into_diagnostic()?;
    assert_eq!(
        leaves.len(),
        3,
        "smoke, the feature-gated test and the one behind two gates: {leaves:?}"
    );
    let paths: Vec<_> = leaves[0]
        .callees()
        .iter()
        .map(|c| c.path().as_str())
        .collect();
    assert_eq!(paths, ["leafy::helper"]);
    assert_eq!(leaves[0].called().len(), 1);
    assert!(leaves[0].called()[0].file().ends_with("src/lib.rs"));
    Ok(())
}

#[test]
fn a_test_function_is_recorded_with_what_it_calls_from_copies() -> Result<()> {
    let _serial = SERIAL.lock().unwrap();
    let copies = Copies::default();
    a_test_function_is_recorded_with_what_it_calls(CfgContextMode::Copies, &copies)?;
    assert!(copies.0.load(Ordering::SeqCst) > 0, "the leaf contexts are copies of the crates");
    Ok(())
}

#[test]
fn a_test_function_is_recorded_with_what_it_calls_from_the_flipped_crates() -> Result<()> {
    let _serial = SERIAL.lock().unwrap();
    let copies = Copies::default();
    a_test_function_is_recorded_with_what_it_calls(CfgContextMode::Flips, &copies)?;
    assert_eq!(copies.0.load(Ordering::SeqCst), 0, "no leaf context copies a crate or a file");
    Ok(())
}

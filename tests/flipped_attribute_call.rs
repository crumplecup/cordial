//! A context that turns off an attribute macro in a dependency does not make the walk of a caller expand the call the
//! macro no longer has.
//!
//! The same shape as a workspace whose crates gate `#[tracing::instrument]` with `cfg_attr(not(kani), ...)`: a crate `dep`
//! whose function is made by an attribute macro until the context sets `kani`, and a crate `user` that re-exports it and
//! calls it from a method that an attribute macro made whatever the cfg is.

use cordial::{
    CfgContextMode, CfgPlanning, CrateTarget, SemanticCargoOptions, SemanticContextKey, SourceDatabase,
};
use miette::{IntoDiagnostic, Result};
use std::sync::{Arc, Mutex};
use tracing_subscriber::Layer as _;
use tracing_subscriber::layer::SubscriberExt;

static SERIAL: Mutex<()> = Mutex::new(());

const MACROS: &str = r#"
extern crate proc_macro;
use proc_macro::TokenStream;

/// Puts a statement at the start of the function's body, as `tracing::instrument` does.
#[proc_macro_attribute]
pub fn wrap(_arguments: TokenStream, item: TokenStream) -> TokenStream {
    item.to_string().replacen('{', "{ let _guard = (); ", 1).parse().expect("valid items")
}

/// A getter for each field of a struct whose fields are all `u8`, as `derive_getters` writes.
#[proc_macro_derive(Accessors)]
pub fn accessors(input: TokenStream) -> TokenStream {
    let text = input.to_string();
    let after = text.split("struct ").nth(1).expect("a struct");
    let name = after.split(|c: char| !c.is_alphanumeric() && c != '_').next().expect("a name");
    let body = after.split('{').nth(1).expect("fields").split('}').next().expect("fields");
    let getters: String = body
        .split(',')
        .filter_map(|field| field.split(':').next())
        .map(str::trim)
        .filter(|field| !field.is_empty())
        .map(|field| format!("pub fn get_{field}(&self) -> u8 {{ self.{field} }}"))
        .collect();
    format!("impl {name} {{ {getters} }}").parse().expect("valid items")
}
"#;

const DEP: &str = "\
use macros::Accessors;

/// What a registered export says.
#[derive(Clone, Debug, Accessors)]
pub struct Snapshot {
    a: u8,
    b: u8,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self { a: 1, b: 2 }
    }
}

/// Collect every registered export into owned values.
///
/// The result is sorted so that tooling can consume it deterministically.
#[cfg_attr(not(kani), macros::wrap)]
pub fn witness() -> Vec<Snapshot> {
    let mut all: Vec<Snapshot> = vec![Snapshot::default(), Snapshot::default()];
    all.sort_by_key(|snapshot| snapshot.get_a());
    all
}
";

const USER: &str = "\
pub use dep::{Snapshot, witness};

mod registry_dump;

pub use registry_dump::run;

#[cfg(kani)]
pub fn harness() -> usize { run() }
";

/// The child module of the dependent crate that calls the function through the crate's own re-export of it.
const REGISTRY_DUMP: &str = "\
use crate::{Snapshot, witness};

pub struct Registry {
    records: Vec<u8>,
}

impl Registry {
    /// Collect what the registry holds.
    #[macros::wrap]
    pub fn collect() -> Registry {
        Registry {
            records: witness().into_iter().map(|snapshot: Snapshot| snapshot.get_a() + snapshot.get_b()).collect(),
        }
    }

    pub fn count(&self) -> usize {
        self.records.len()
    }
}

#[macros::wrap]
pub fn run_dump() -> usize {
    Registry::collect().count()
}

pub fn run() -> usize { run_dump() }
";

fn specimen() -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    for directory in ["src", "macros/src", "dep/src"] {
        std::fs::create_dir_all(root.path().join(directory)).into_diagnostic()?;
    }
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname = \"user\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\
         [dependencies]\nmacros = { path = \"macros\" }\ndep = { path = \"dep\" }\n[workspace]\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        root.path().join("macros/Cargo.toml"),
        "[package]\nname = \"macros\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[lib]\nproc-macro = true\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        root.path().join("dep/Cargo.toml"),
        "[package]\nname = \"dep\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\
         [dependencies]\nmacros = { path = \"../macros\" }\n",
    )
    .into_diagnostic()?;
    std::fs::write(root.path().join("macros/src/lib.rs"), MACROS).into_diagnostic()?;
    std::fs::write(root.path().join("dep/src/lib.rs"), DEP).into_diagnostic()?;
    std::fs::write(root.path().join("src/lib.rs"), USER).into_diagnostic()?;
    std::fs::write(root.path().join("src/registry_dump.rs"), REGISTRY_DUMP).into_diagnostic()?;
    Ok(root)
}

/// What each crate said it has, in the phase it said it: the name, the phase, how many attribute macro calls its map
/// declares and how many the context removed.
type CrateSaid = (String, String, u64, u64);

#[derive(Clone, Default)]
struct Crates(Arc<Mutex<Vec<CrateSaid>>>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Crates {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        #[derive(Default)]
        struct Fields {
            phase: String,
            krate: String,
            calls: u64,
            removed: u64,
        }
        impl tracing::field::Visit for Fields {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                match field.name() {
                    "phase" => self.phase = format!("{value:?}").trim_matches('"').to_owned(),
                    "krate" => self.krate = format!("{value:?}"),
                    _ => {}
                }
            }
            fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
                match field.name() {
                    "attr_macro_calls" => self.calls = value,
                    "calls_removed_since_ordinary" => self.removed = value,
                    _ => {}
                }
            }
        }
        if event.metadata().target() == "cordial::semantic::flip" {
            let mut fields = Fields::default();
            event.record(&mut fields);
            self.0.lock().unwrap().push((fields.krate, fields.phase, fields.calls, fields.removed));
        }
    }
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_context_that_turns_off_an_attribute_macro_in_a_dependency_does_not_expand_its_call_for_a_caller() -> Result<()> {
    let _serial = SERIAL.lock().unwrap();
    cordial::init_tracing();
    let root = specimen()?;
    let options = SemanticCargoOptions::default()
        .with_cfg_contexts(vec![SemanticContextKey::enabling(["kani"])])
        .with_cfg_planning(CfgPlanning::Explicit)
        .with_cfg_context_mode(CfgContextMode::Flips);
    let crates = Crates::default();
    // What `RUST_LOG` asks for is said to the terminal beside what the test collects, and nothing is without it.
    let subscriber = tracing_subscriber::registry()
        .with(crates.clone())
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr).with_filter(tracing_subscriber::EnvFilter::from_default_env()));
    tracing::subscriber::with_default(subscriber, || -> Result<()> {
        let mut session = SourceDatabase::load_with(root.path(), &options).into_diagnostic()?;
        session
            .project(&CrateTarget::new("user", root.path()), &[root.path().join("src/lib.rs")])
            .into_diagnostic()?;
        Ok(())
    })?;
    // Not a vacuous pass: the macro made `witness` in `dep` before the context, and the context took the call away.
    let said = crates.0.lock().unwrap().clone();
    let of = |krate: &str, phase: &str| said.iter().find(|(name, said_phase, ..)| name == krate && said_phase.starts_with(phase));
    assert_eq!(of("dep", "ordinary").map(|said| said.2), Some(1), "the macro applies in `dep` before the context: {said:?}");
    assert_eq!(of("dep", "flipped").map(|said| (said.2, said.3)), Some((0, 1)), "the context takes the call away: {said:?}");
    Ok(())
}

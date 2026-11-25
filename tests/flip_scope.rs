//! A flipped pass is over the crates the target reaches, not every crate of the workspace.

use cordial::{
    CfgContextMode, CfgPlanning, CrateTarget, SemanticCargoOptions, SemanticContextKey, SourceDatabase,
};
use miette::{IntoDiagnostic, Result};
use std::sync::{Arc, Mutex};
use tracing_subscriber::layer::SubscriberExt;

static SERIAL: Mutex<()> = Mutex::new(());

/// `a` depends on `b`, and `c` is on its own; each has a function only the context `kani` has.
fn workspace() -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    let write = |path: &str, text: &str| -> Result<()> {
        let path = root.path().join(path);
        std::fs::create_dir_all(path.parent().expect("a file has a directory")).into_diagnostic()?;
        std::fs::write(path, text).into_diagnostic()
    };
    write("Cargo.toml", "[workspace]\nmembers = [\"a\", \"b\", \"c\"]\nresolver = \"2\"\n")?;
    for (name, dependency) in [("a", "b = { path = \"../b\" }\n"), ("b", ""), ("c", "")] {
        write(
            &format!("{name}/Cargo.toml"),
            &format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[dependencies]\n{dependency}"),
        )?;
    }
    write("a/src/lib.rs", "pub fn from_a() -> u32 { b::from_b() }\n#[cfg(kani)]\npub fn proof_a() {}\n")?;
    write("b/src/lib.rs", "pub fn from_b() -> u32 { 1 }\n#[cfg(kani)]\npub fn proof_b() {}\n")?;
    write("c/src/lib.rs", "pub fn from_c() -> u32 { 2 }\n#[cfg(kani)]\npub fn proof_c() {}\n")?;
    Ok(root)
}

/// What a pass said of its crates: `reached` with how many of how many, or `written` with how many.
type Said = (&'static str, u64, u64);

/// What the passes said of the crates they were over, in the order the passes ran.
#[derive(Clone, Default)]
struct Reach(Arc<Mutex<Vec<Said>>>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Reach {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        #[derive(Default)]
        struct Fields([Option<u64>; 3]);
        impl tracing::field::Visit for Fields {
            fn record_debug(&mut self, _: &tracing::field::Field, _: &dyn std::fmt::Debug) {}
            fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
                match field.name() {
                    "reached" => self.0[0] = Some(value),
                    "analysed" => self.0[1] = Some(value),
                    "crates" => self.0[2] = Some(value),
                    _ => {}
                }
            }
        }
        if event.metadata().target() != "cordial::semantic::contexts" {
            return;
        }
        let mut fields = Fields::default();
        event.record(&mut fields);
        let mut said = self.0.lock().unwrap();
        match fields.0 {
            [Some(reached), Some(analysed), _] => said.push(("reached", reached, analysed)),
            [None, None, Some(written)] => said.push(("written", written, 0)),
            _ => {}
        }
    }
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_target_flips_the_crates_it_holds_and_depends_on_and_not_the_rest_of_the_workspace() -> Result<()> {
    let _serial = SERIAL.lock().unwrap();
    cordial::init_tracing();
    let root = workspace()?;
    let options = SemanticCargoOptions::default()
        .with_cfg_contexts(vec![SemanticContextKey::enabling(["kani"])])
        .with_cfg_planning(CfgPlanning::Explicit)
        .with_cfg_context_mode(CfgContextMode::Flips);
    let reach = Reach::default();
    let subscriber = tracing_subscriber::registry().with(reach.clone());
    tracing::subscriber::with_default(subscriber, || -> Result<()> {
        let mut session = SourceDatabase::load_saved(root.path(), &options, None).into_diagnostic()?;
        for name in ["a", "c"] {
            let dir = root.path().join(name);
            session
                .project(&CrateTarget::new(name, &dir), &[dir.join("src/lib.rs")])
                .into_diagnostic()?;
        }
        Ok(())
    })?;
    let said = reach.0.lock().unwrap().clone();
    // The pass for `a` is over `a` and `b`, which it depends on; the pass for `c` is over `c` alone.
    assert_eq!(
        said,
        [("reached", 2, 3), ("written", 2, 0), ("reached", 1, 3), ("written", 1, 0)],
        "reached of how many, and the crates a context's cfg was written into, for `a` and then `c`"
    );
    Ok(())
}

/// What a harvest re-validated: how many memos, and how many of them were of low, medium and high durability.
type Verified = (u64, [u64; 3]);

/// What each pass said it re-validated in its harvest.
#[derive(Clone, Default)]
struct Harvests(Arc<Mutex<Vec<Verified>>>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Harvests {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        #[derive(Default)]
        struct Fields {
            label: String,
            memos: u64,
            by: [u64; 3],
        }
        impl tracing::field::Visit for Fields {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                if field.name() == "label" {
                    self.label = format!("{value:?}");
                }
            }
            fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
                match field.name() {
                    "verified_memos" => self.memos = value,
                    "verified_low" => self.by[0] = value,
                    "verified_medium" => self.by[1] = value,
                    "verified_high" => self.by[2] = value,
                    _ => {}
                }
            }
        }
        if event.metadata().target() == "cordial::segment" {
            let mut fields = Fields::default();
            event.record(&mut fields);
            if fields.label == "projection/contexts/context/harvest" {
                self.0.lock().unwrap().push((fields.memos, fields.by));
            }
        }
    }
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn the_first_flip_of_a_crates_cfg_does_not_make_the_library_verify_again() -> Result<()> {
    let _serial = SERIAL.lock().unwrap();
    cordial::init_tracing();
    let root = workspace()?;
    let options = SemanticCargoOptions::default()
        .with_cfg_contexts(vec![SemanticContextKey::enabling(["kani"])])
        .with_cfg_planning(CfgPlanning::Explicit)
        .with_cfg_context_mode(CfgContextMode::Flips);
    let harvests = Harvests::default();
    let subscriber = tracing_subscriber::registry().with(harvests.clone());
    tracing::subscriber::with_default(subscriber, || -> Result<()> {
        let mut session = SourceDatabase::load_saved(root.path(), &options, None).into_diagnostic()?;
        // `a` and `b` are flipped for the first time by the first pass, `c` by the second, and `a` again by the third.
        for name in ["a", "c", "a"] {
            let dir = root.path().join(name);
            session
                .project(&CrateTarget::new(name, &dir), &[dir.join("src/lib.rs")])
                .into_diagnostic()?;
        }
        Ok(())
    })?;
    let harvests = harvests.0.lock().unwrap().clone();
    assert_eq!(harvests.len(), 3, "one harvest for each pass: {harvests:?}");
    // The library's memos (the sysroot's impls, attributes and item trees) are of medium durability, and the cfg of
    // a local crate is of low: writing it must not be a write that the library's memos have to be verified against.
    // Written at medium, the first write to each crate cost about 113,000 memos, over and over again.
    for (memos, by_durability) in &harvests {
        assert_eq!(by_durability[1], 0, "no memo of medium durability is verified by a flip of a local cfg: {harvests:?}");
        assert!(*memos < 5_000, "a pass over a few functions verifies a few memos, not the library: {harvests:?}");
    }
    Ok(())
}

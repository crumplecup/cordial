//! Regions of a run say what they cost, whatever the mode, so that two runs are compared region by region.

use cordial::{
    CfgContextMode, CfgPlanning, CrateTarget, Segment, SemanticCargoOptions, SemanticContextKey, SourceDatabase,
};
use miette::{IntoDiagnostic, Result};
use std::sync::{Arc, Mutex};
use tracing_subscriber::layer::SubscriberExt;

static SERIAL: Mutex<()> = Mutex::new(());

/// What a segment said when it ended: its path, the scope it was in and how long it took.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Said {
    label: String,
    scope: String,
    micros: u64,
}

#[derive(Clone, Default)]
struct Segments(Arc<Mutex<Vec<Said>>>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Segments {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        #[derive(Default)]
        struct Fields(String, String, u64);
        impl tracing::field::Visit for Fields {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                match field.name() {
                    "label" => self.0 = format!("{value:?}"),
                    "scope" => self.1 = format!("{value:?}"),
                    _ => {}
                }
            }
            fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
                if field.name() == "micros" {
                    self.2 = value;
                }
            }
        }
        if event.metadata().target() == "cordial::segment" {
            let mut fields = Fields::default();
            event.record(&mut fields);
            self.0.lock().unwrap().push(Said { label: fields.0, scope: fields.1, micros: fields.2 });
        }
    }
}

#[test]
fn a_segment_inside_another_has_its_name_in_front_and_ends_first_and_says_the_scope() {
    let _serial = SERIAL.lock().unwrap();
    let segments = Segments::default();
    let subscriber = tracing_subscriber::registry().with(segments.clone());
    tracing::subscriber::with_default(subscriber, || {
        let _scope = Segment::scope("package");
        let _outer = Segment::begin("outer");
        {
            let _inner = Segment::begin("inner");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let _second = Segment::begin("second");
    });
    let said = segments.0.lock().unwrap().clone();
    let labels: Vec<&str> = said.iter().map(|said| said.label.as_str()).collect();
    assert_eq!(labels, ["outer/inner", "outer/second", "outer"], "inner ones end before the one they are in");
    assert!(said.iter().all(|said| said.scope == "package"), "{said:?}");
    assert!(said[0].micros >= 5_000, "the sleep is in the inner one: {said:?}");
    assert!(said[2].micros >= said[0].micros, "the outer one holds the inner one: {said:?}");
}

/// A crate with a gate the context `kani` turns on, in `mode`, and what its run said of its regions.
fn regions_of_a_run(mode: CfgContextMode) -> Result<Vec<Said>> {
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir_all(root.path().join("src")).into_diagnostic()?;
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='gated'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        root.path().join("src/lib.rs"),
        "pub fn alive() -> u32 { 1 }\n#[cfg(kani)]\npub fn harness() -> u32 { alive() }\n",
    )
    .into_diagnostic()?;
    let options = SemanticCargoOptions::default()
        .with_cfg_contexts(vec![SemanticContextKey::enabling(["kani"])])
        .with_cfg_planning(CfgPlanning::Explicit)
        .with_cfg_context_mode(mode);
    let segments = Segments::default();
    let subscriber = tracing_subscriber::registry().with(segments.clone());
    tracing::subscriber::with_default(subscriber, || -> Result<()> {
        let mut session = SourceDatabase::load_saved(root.path(), &options, None).into_diagnostic()?;
        session
            .project(&CrateTarget::new("gated", root.path()), &[root.path().join("src/lib.rs")])
            .into_diagnostic()?;
        Ok(())
    })?;
    let said = segments.0.lock().unwrap().clone();
    Ok(said)
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn every_region_of_the_context_pass_says_what_it_cost_in_both_modes() -> Result<()> {
    let _serial = SERIAL.lock().unwrap();
    cordial::init_tracing();
    for mode in [CfgContextMode::Copies, CfgContextMode::Flips] {
        let said = regions_of_a_run(mode)?;
        let labels: std::collections::BTreeSet<&str> = said.iter().map(|said| said.label.as_str()).collect();
        // What both modes do for each context, under the projection of the package.
        for expected in [
            "projection",
            "projection/contexts",
            "projection/contexts/context",
            "projection/contexts/context/enter",
            "projection/contexts/context/harvest",
            "projection/contexts/context/end",
            "projection/contexts/context/kept",
            "projection/save",
        ] {
            assert!(labels.contains(expected), "{mode:?} says nothing of `{expected}`: {labels:#?}");
        }
        assert!(said.iter().all(|said| said.scope == "gated"), "the package is said in every region: {said:?}");
        let flips = [
            "projection/contexts/context/enter/flip_write",
            "projection/contexts/finish_flips",
            "projection/contexts/finish_flips/restore",
            "projection/contexts/finish_flips/forget_flipped",
            "projection/contexts/finish_flips/forget_changed",
            "projection/contexts/finish_flips/settle_flipped",
            "projection/contexts/finish_flips/settle_flipped/settle",
            "projection/contexts/finish_flips/settle_flipped/settle/maps",
            "projection/contexts/finish_flips/settle_flipped/settle/bodies",
            "projection/contexts/finish_flips/settle_flipped/settle/blocks",
            "projection/contexts/finish_flips/settle_flipped/forget_undeclared",
        ];
        for expected in flips {
            assert_eq!(
                labels.contains(expected),
                mode == CfgContextMode::Flips,
                "`{expected}` is said by flips and not by copies; {mode:?}: {labels:#?}"
            );
        }
    }
    Ok(())
}

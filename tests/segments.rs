//! Regions of a run say what they cost, whatever the mode, so that two runs are compared region by region.

use cordial::{
    CfgContextMode, CfgPlanning, CrateTarget, Segment, SemanticCargoOptions, SemanticContextKey, SourceDatabase,
};
use miette::{IntoDiagnostic, Result};
use std::sync::{Arc, Mutex};
use tracing_subscriber::Layer as _;
use tracing_subscriber::layer::SubscriberExt;

static SERIAL: Mutex<()> = Mutex::new(());

/// What a segment said when it ended: its path, the scope it was in and how long it took.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Said {
    label: String,
    scope: String,
    micros: u64,
    queries: u64,
    verified_memos: u64,
    verified_edges: u64,
    /// The queries whose memos it re-validated most, as `name=memos/edges`.
    verified_top: String,
    /// How many memos of low, medium and high durability it re-validated.
    by_durability: [u64; 3],
    /// The queries it executed most, as `name=count`.
    executed_top: String,
    /// How many keyed reads it made, and how many keys it touched (see `salsa::keyed`).
    keyed: [u64; 2],
}

#[derive(Clone, Default)]
struct Segments(Arc<Mutex<Vec<Said>>>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Segments {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        #[derive(Default)]
        struct Fields {
            label: String,
            scope: String,
            verified_top: String,
            executed_top: String,
            by_durability: [u64; 3],
            /// micros, queries, verified memos, verified edges, keyed reads, keys touched.
            numbers: [u64; 6],
        }
        impl tracing::field::Visit for Fields {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                match field.name() {
                    "label" => self.label = format!("{value:?}"),
                    "scope" => self.scope = format!("{value:?}"),
                    "verified_top" => self.verified_top = format!("{value:?}").trim_matches('"').to_owned(),
                    "top" => self.executed_top = format!("{value:?}").trim_matches('"').to_owned(),
                    _ => {}
                }
            }
            fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
                let by = ["verified_low", "verified_medium", "verified_high"]
                    .iter()
                    .position(|name| *name == field.name());
                if let Some(by) = by {
                    self.by_durability[by] = value;
                }
                let at = ["micros", "queries", "verified_memos", "verified_edges", "keyed_reads", "keyed_touches"]
                    .iter()
                    .position(|name| *name == field.name());
                if let Some(at) = at {
                    self.numbers[at] = value;
                }
            }
        }
        if event.metadata().target() == "cordial::segment" {
            let mut fields = Fields::default();
            event.record(&mut fields);
            let [micros, queries, verified_memos, verified_edges, keyed_reads, keyed_touches] = fields.numbers;
            self.0.lock().unwrap().push(Said {
                label: fields.label,
                scope: fields.scope,
                micros,
                queries,
                verified_memos,
                verified_edges,
                verified_top: fields.verified_top,
                by_durability: fields.by_durability,
                executed_top: fields.executed_top,
                keyed: [keyed_reads, keyed_touches],
            });
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
    regions_of_a_crate(
        mode,
        "pub fn alive() -> u32 { 1 }\n#[cfg(kani)]\npub fn harness() -> u32 { alive() }\n",
    )
}

/// The regions a run of the context `kani` says of the crate whose root is `library`, in `mode`.
fn regions_of_a_crate(mode: CfgContextMode, library: &str) -> Result<Vec<Said>> {
    regions_of(mode, SemanticContextKey::enabling(["kani"]), &[("src/lib.rs", library)])
}

/// The regions a run of `context` says of the crate with these files (the first is its root), in `mode`, with the
/// queries it executed counted.
fn regions_of(mode: CfgContextMode, context: SemanticContextKey, files: &[(&str, &str)]) -> Result<Vec<Said>> {
    Ok(regions_and_forgets(mode, context, files)?.0)
}

/// What a forget dropped: how many memos, and the queries most.
type Forgotten = (u64, String);

/// [`regions_of`], and what each forget of the run dropped.
fn regions_and_forgets(
    mode: CfgContextMode,
    context: SemanticContextKey,
    files: &[(&str, &str)],
) -> Result<(Vec<Said>, Vec<Forgotten>)> {
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir_all(root.path().join("src")).into_diagnostic()?;
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='gated'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    for (path, text) in files {
        let path = root.path().join(path);
        std::fs::create_dir_all(path.parent().expect("a file has a directory")).into_diagnostic()?;
        std::fs::write(path, text).into_diagnostic()?;
    }
    let options = SemanticCargoOptions::default()
        .with_cfg_contexts(vec![context])
        .with_cfg_planning(CfgPlanning::Explicit)
        .with_cfg_context_mode(mode);
    let segments = Segments::default();
    let forgets = Forgets::default();
    // The live layer counts the queries Salsa executes, which a region says it ran.
    let subscriber = tracing_subscriber::registry()
        .with(segments.clone())
        .with(forgets.clone())
        .with(cordial::LiveLayer.with_filter(cordial::live_filter()));
    tracing::subscriber::with_default(subscriber, || -> Result<()> {
        let mut session = SourceDatabase::load_saved(root.path(), &options, None).into_diagnostic()?;
        session
            .project(&CrateTarget::new("gated", root.path()), &[root.path().join("src/lib.rs")])
            .into_diagnostic()?;
        Ok(())
    })?;
    let said = segments.0.lock().unwrap().clone();
    let forgotten = forgets.0.lock().unwrap().clone();
    Ok((said, forgotten))
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
            "projection/finish_flips",
            "projection/finish_flips/restore",
            "projection/finish_flips/forget_flipped",
            "projection/finish_flips/forget_changed",
            "projection/finish_flips/settle_flipped",
            "projection/finish_flips/settle_flipped/settle",
            "projection/finish_flips/settle_flipped/settle/maps",
            "projection/finish_flips/settle_flipped/settle/bodies",
            "projection/finish_flips/settle_flipped/settle/blocks",
            "projection/finish_flips/settle_flipped/forget_undeclared",
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

/// A crate of `functions` plain functions, each calling a macro, and a function only the context `kani` has.
fn plain_crate(functions: usize) -> String {
    let mut text = String::from("macro_rules! twice { ($e:expr) => { $e + $e }; }\n");
    for n in 0..functions {
        text.push_str(&format!("pub fn plain_{n}() -> u32 {{ twice!({n}) }}\n"));
    }
    text.push_str("#[cfg(kani)]\npub fn harness() -> u32 { 1 }\n");
    text
}

fn totals(said: &[Said], label: &str) -> (u64, u64, u64) {
    said.iter()
        .filter(|said| said.label == label)
        .fold((0, 0, 0), |sum, said| (sum.0 + said.queries, sum.1 + said.verified_memos, sum.2 + said.verified_edges))
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_flip_that_changes_nothing_verifies_memos_in_proportion_to_the_crate_and_not_the_library() -> Result<()> {
    let _serial = SERIAL.lock().unwrap();
    cordial::init_tracing();
    let harvest = "projection/contexts/context/harvest";
    let small = totals(&regions_of_a_crate(CfgContextMode::Flips, &plain_crate(5))?, harvest).1;
    let large = totals(&regions_of_a_crate(CfgContextMode::Flips, &plain_crate(50))?, harvest).1;
    // The library's memos (about 113,000 of them) were verified by the first write of a local crate's cfg, which was
    // a write at medium durability: five functions cost 112,800 memos and fifty cost 113,610.
    assert!(small < 500, "a crate of 5 functions has its own memos verified, not the library's: {small}");
    assert!(
        large - small <= 25 * 45,
        "each of the 45 functions more costs a few memos, and not a share of the library: {small} for 5, {large} for 50"
    );
    Ok(())
}

/// What each forget of a flipped pass dropped: how many, and the queries most.
#[derive(Clone, Default)]
struct Forgets(Arc<Mutex<Vec<Forgotten>>>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Forgets {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        #[derive(Default)]
        struct Fields {
            forgotten: Option<u64>,
            top: Option<String>,
        }
        impl tracing::field::Visit for Fields {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                if field.name() == "top" {
                    self.top = Some(format!("{value:?}").trim_matches('"').to_owned());
                }
            }
            fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
                if field.name() == "forgotten" {
                    self.forgotten = Some(value);
                }
            }
        }
        if event.metadata().target() == "cordial::eviction" {
            let mut fields = Fields::default();
            event.record(&mut fields);
            if let (Some(forgotten), Some(top)) = (fields.forgotten, fields.top) {
                self.0.lock().unwrap().push((forgotten, top));
            }
        }
    }
}

/// A crate with a `#[cfg(test)]` module and an integration-test file, so that the context pass and the leaf pass both
/// flip `test`.
fn crate_with_unit_and_leaf_tests() -> [(&'static str, String); 2] {
    [
        (
            "src/lib.rs",
            String::from(
                "pub fn plain() -> u32 { 1 }\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn unit() { assert_eq!(super::plain(), 1); }\n}\n",
            ),
        ),
        ("tests/leaf.rs", String::from("#[test]\nfn leaf() { assert_eq!(gated::plain(), 1); }\n")),
    ]
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn the_context_pass_and_the_leaf_pass_are_one_flipped_session() -> Result<()> {
    let _serial = SERIAL.lock().unwrap();
    cordial::init_tracing();
    let files = crate_with_unit_and_leaf_tests();
    let borrowed: Vec<(&str, &str)> = files.iter().map(|(path, text)| (*path, text.as_str())).collect();
    let said = regions_of(CfgContextMode::Flips, SemanticContextKey::enabling(["test"]), &borrowed)?;
    let finishes: Vec<&str> =
        said.iter().map(|said| said.label.as_str()).filter(|label| label.ends_with("/finish_flips")).collect();
    // The cfg is put back, what the flips changed forgotten and the maps settled once, when the last flip is done:
    // the flipped state the context pass leaves is the one the leaf pass starts from, and written over it.
    assert_eq!(finishes.len(), 1, "one finish for the whole flipped session: {finishes:?}");
    let leaf_flips = said.iter().filter(|said| said.label.ends_with("leaf/enter/flip_write")).count();
    assert!(leaf_flips >= 1, "the leaf stage still flips its contexts");
    Ok(())
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_region_says_how_many_names_it_read_by_key_and_how_many_keys_it_touched() -> Result<()> {
    let _serial = SERIAL.lock().unwrap();
    cordial::init_tracing();
    let said = regions_of_a_run(CfgContextMode::Flips)?;
    let reads: u64 = said.iter().filter(|said| said.label == "projection").map(|said| said.keyed[0]).sum();
    let touches: u64 = said.iter().filter(|said| said.label == "projection").map(|said| said.keyed[1]).sum();
    assert!(reads > 0, "resolving names reads them by key: {said:?}");
    // The context adds `harvest`'s gate and so one name to the crate's scope, and the map made again says so.
    assert!(touches > 0, "a flip that adds a name touches its key: {said:?}");
    Ok(())
}

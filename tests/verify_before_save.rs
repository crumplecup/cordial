//! What verifying the persisted memos before a save has to run again. A memo is saved as valid
//! now, so the memos the cfg contexts left behind are walked once more. Nothing they read has
//! changed, so nothing should run again: before the keys the per-file queries are made with were
//! immortal, every context's input writes aged them out and the save ran the queries again.

use cordial::{
    CfgPlanning, CrateTarget, LiveLayer, MetricsLayer, SavedState, SemanticCargoOptions,
    SemanticContextKey, SourceDatabase, StoreLayout, live_filter, metrics_filter_with,
};
use miette::{IntoDiagnostic, Result};
use tracing_subscriber::{Layer, layer::SubscriberExt};

const FILES: usize = 4;

/// What Salsa said about inputs written and memos found out of date, counted by what they were.
#[derive(Default, Clone)]
struct Writes(std::sync::Arc<std::sync::Mutex<std::collections::BTreeMap<String, u64>>>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Writes {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        let target = event.metadata().target();
        if target != "salsa::write" && target != "salsa::stale" {
            return;
        }
        #[derive(Default)]
        struct Fields(std::collections::BTreeMap<&'static str, String>);
        impl tracing::field::Visit for Fields {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                self.0.insert(
                    field.name(),
                    format!("{value:?}").trim_matches('"').to_owned(),
                );
            }
        }
        let mut fields = Fields::default();
        event.record(&mut fields);
        let get = |name: &str| fields.0.get(name).cloned().unwrap_or_default();
        let name = if target == "salsa::write" {
            format!(
                "write {}.{} [{}]",
                get("input"),
                get("field"),
                get("durability")
            )
        } else {
            format!("stale {} <- {}", get("memo"), get("changed"))
        };
        if let Ok(mut counts) = self.0.lock() {
            *counts.entry(name).or_default() += 1;
        }
    }
}

fn module(index: usize, contexts: usize) -> String {
    // Constructs the per-file queries look at: types, a trait and its impl, derives, an error type,
    // panics and printing, a macro, a closure, a test module, a constant.
    let mut text = format!(
        "use std::fmt;\n\
         #[derive(Debug, Clone, PartialEq)]\n\
         pub struct Widget{index} {{ pub size: u32 }}\n\
         #[derive(Debug)]\n\
         pub enum Failure{index} {{ Missing, Broken(String) }}\n\
         impl fmt::Display for Failure{index} {{\n\
             fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {{ write!(f, \"failure\") }}\n\
         }}\n\
         impl std::error::Error for Failure{index} {{}}\n\
         pub trait Shape{index} {{ fn area(&self) -> u32; fn describe(&self) -> String {{ format!(\"{{}}\", self.area()) }} }}\n\
         impl Shape{index} for Widget{index} {{ fn area(&self) -> u32 {{ self.size * 2 }} }}\n\
         impl Widget{index} {{ pub fn new() -> Self {{ Self {{ size: 0 }} }} }}\n\
         macro_rules! twice{index} {{ ($e:expr) => {{ $e + $e }}; }}\n\
         pub const LIMIT{index}: u32 = 10;\n\
         pub fn helper{index}() -> Widget{index} {{ Widget{index}::new() }}\n\
         pub fn risky{index}(value: Option<u32>) -> Result<u32, Failure{index}> {{\n\
             let doubled = value.map(|v| twice{index}!(v));\n\
             println!(\"{{:?}}\", doubled);\n\
             if doubled.is_none() {{ return Err(Failure{index}::Missing); }}\n\
             Ok(doubled.unwrap())\n\
         }}\n\
         pub fn fails{index}(text: &str) -> u32 {{ text.parse::<u32>().expect(\"a number\") }}\n\
         #[cfg(test)]\n\
         mod tests{index} {{ #[test] fn adds() {{ assert_eq!(1 + 1, 2); }} }}\n"
    );
    for context in 0..contexts {
        text.push_str(&format!(
            "#[cfg(c{context})]\npub fn gated_{index}_{context}() -> Widget{index} {{ helper{index}() }}\n"
        ));
    }
    text
}

fn fixture(contexts: usize) -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir_all(root.path().join("src")).into_diagnostic()?;
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='grown'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    let mods: String = (0..FILES).map(|i| format!("pub mod m{i};\n")).collect();
    std::fs::write(root.path().join("src/lib.rs"), mods).into_diagnostic()?;
    for index in 0..FILES {
        std::fs::write(
            root.path().join(format!("src/m{index}.rs")),
            module(index, contexts),
        )
        .into_diagnostic()?;
    }
    Ok(root)
}

/// What the verification before the save did, and what there was to verify.
struct Verified {
    memos: u64,
    queries: u64,
    top: String,
    analysed: u64,
    /// The persisted queries that have memos.
    kinds: std::collections::BTreeSet<String>,
}

/// Project the fixture with a place to save, and say what the verification before the save did.
fn verification(contexts: usize) -> Result<Verified> {
    verification_seen(contexts, &Writes::default())
}

fn verification_seen(contexts: usize, writes: &Writes) -> Result<Verified> {
    let root = fixture(contexts)?;
    let keys = (0..contexts)
        .map(|context| SemanticContextKey::enabling([format!("c{context}")]))
        .collect();
    let options = SemanticCargoOptions::default()
        .with_cfg_contexts(keys)
        .with_cfg_planning(CfgPlanning::Explicit);
    let saved = SavedState::at(root.path().join("state"));
    let subscriber = tracing_subscriber::registry()
        .with(MetricsLayer.with_filter(metrics_filter_with(false)))
        .with(LiveLayer.with_filter(live_filter()))
        .with(
            writes.clone().with_filter(
                tracing_subscriber::filter::Targets::new()
                    .with_target("salsa::write", tracing::Level::DEBUG)
                    .with_target("salsa::stale", tracing::Level::DEBUG),
            ),
        );
    tracing::subscriber::with_default(subscriber, || -> Result<()> {
        let mut session =
            SourceDatabase::load_saved(root.path(), &options, Some(saved)).into_diagnostic()?;
        session
            .project(
                &CrateTarget::new("grown", root.path()),
                &[root.path().join("src/lib.rs")],
            )
            .into_diagnostic()?;
        Ok(())
    })?;
    let store = StoreLayout::from_root(root.path().join("store"), "specimen".to_owned());
    cordial::write_run_metrics(&store).into_diagnostic()?;
    let metrics = cordial::read_run_metrics(&store);
    let value = |name: &str| metrics.values().get(name).copied().unwrap_or(0);
    let kinds = metrics
        .values()
        .iter()
        .filter(|(_, memos)| **memos > 0)
        .filter_map(|(name, _)| {
            name.strip_prefix("persistence.")?
                .strip_suffix(".memos")
                .map(str::to_owned)
        })
        .collect();
    let top = metrics
        .notes()
        .get("cache.verified.top")
        .cloned()
        .unwrap_or_default();
    Ok(Verified {
        memos: value("cache.verified.memos"),
        queries: value("cache.verified.queries"),
        top,
        analysed: value("milestones.cfg_context_entered.events"),
        kinds,
    })
}

#[test]
fn verifying_with_no_context_has_nothing_to_run_again() -> Result<()> {
    cordial::init_tracing();
    let verified = verification(0)?;
    assert_eq!(verified.analysed, 0);
    assert_eq!(
        (verified.memos, verified.queries),
        (0, 0),
        "[{}]",
        verified.top
    );
    Ok(())
}

/// The contexts a session goes through before it saves. Each one's input writes advance the
/// revision, so what an earlier context's per-file queries were made with must outlive them all.
const CONTEXTS: usize = 12;

/// The persisted queries this audits: the files' collectors, the definition maps, and the
/// per-function and per-crate facts. A fixture that stopped making their memos would stop
/// auditing them.
const AUDITED: &[&str] = &[
    "antipattern_facts_file",
    "architecture_file",
    "crate_local_def_map",
    "error_sites_file",
    "observability_file",
    "panic_scan_file",
    "persisted_evidence",
    "policy_file",
    "verus_file",
];

/// Each context's input writes advance the revision. What the earlier contexts' per-file queries
/// were keyed by must outlive that, or the memos that read the keys go out of date and the save's
/// verification runs their queries again: more of them with each context, and unrelated to what
/// changed.
#[test]
fn verifying_after_several_contexts_runs_nothing_again() -> Result<()> {
    cordial::init_tracing();
    let writes = Writes::default();
    let verified = verification_seen(CONTEXTS, &writes)?;
    assert_eq!(verified.analysed, CONTEXTS as u64);
    assert!(verified.memos > 0, "something was there to verify");
    for kind in AUDITED {
        assert!(
            verified.kinds.contains(*kind),
            "{kind} has no memos, so nothing audits it: {:?}",
            verified.kinds
        );
    }
    assert_eq!(
        verified.queries,
        0,
        "verifying ran these again: [{}]; out of date: {:?}",
        verified.top,
        writes.0.lock().expect("a lock")
    );
    // And no memo is found out of date: the contexts changed nothing a memo of an earlier one read.
    let stale: Vec<String> = writes
        .0
        .lock()
        .expect("a lock")
        .keys()
        .filter(|name| name.starts_with("stale"))
        .cloned()
        .collect();
    assert!(
        stale.is_empty(),
        "out of date although nothing it read changed: {stale:?}"
    );
    Ok(())
}

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

/// What the fixture workspace has beyond a library with gated code.
#[derive(Clone, Copy, Default)]
struct Setup {
    /// A procedural macro crate, and a derive from it on a type in each module.
    macros: bool,
    /// An integration test binary, which the leaf pass reads under the `test` cfg.
    tests: bool,
    /// A second package of the workspace that depends on the first and has gates of its own.
    member: bool,
}

/// What Salsa said about inputs written and memos found out of date, counted by what they were.
#[derive(Default, Clone)]
struct Writes(
    std::sync::Arc<std::sync::Mutex<std::collections::BTreeMap<String, u64>>>,
    std::sync::Arc<std::sync::Mutex<Vec<String>>>,
);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Writes {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        let target = event.metadata().target();
        if target != "salsa::write"
            && target != "salsa::stale"
            && target != "cordial::milestones"
            && target != "salsa::explain"
            && target != "salsa::function::execute"
        {
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
        if target == "salsa::function::execute" {
            // "name(args): executing query": counted by name, for what a fixture exercised.
            let message = get("message");
            if let Some(name) = message.split('(').next()
                && let Ok(mut counts) = self.0.lock()
            {
                *counts.entry(format!("exec {name}")).or_default() += 1;
            }
            return;
        }
        if target == "salsa::explain" {
            if let Ok(mut order) = self.1.lock() {
                order.push(format!("explain {} via {}", get("ingredient"), get("path")));
            }
            return;
        }
        if target == "cordial::milestones" {
            if let Ok(mut order) = self.1.lock() {
                order.push(format!("-- {}", get("phase")));
            }
            return;
        }
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
        if let Ok(mut order) = self.1.lock() {
            order.push(name.clone());
        }
        if let Ok(mut counts) = self.0.lock() {
            *counts.entry(name).or_default() += 1;
        }
    }
}

fn module(index: usize, contexts: usize, macros: bool) -> String {
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
    if macros {
        text.push_str(&format!(
            "#[derive(macros::Hello)]\npub struct Derived{index};\n"
        ));
    }
    for context in 0..contexts {
        text.push_str(&format!(
            "#[cfg(c{context})]\npub fn gated_{index}_{context}() -> Widget{index} {{ helper{index}() }}\n"
        ));
    }
    text
}

/// A derive that adds a function, so that expanding it matters, in a crate of its own.
const MACROS: &str = r#"
extern crate proc_macro;
use proc_macro::TokenStream;

#[proc_macro_derive(Hello)]
pub fn hello(_input: TokenStream) -> TokenStream {
    "pub fn hello_generated() {}".parse().expect("valid items")
}
"#;

fn fixture(contexts: usize, setup: Setup) -> Result<tempfile::TempDir> {
    let Setup {
        macros,
        tests,
        member,
    } = setup;
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir_all(root.path().join("src")).into_diagnostic()?;
    std::fs::write(
        root.path().join("Cargo.toml"),
        format!(
            "[package]\nname='grown'\nversion='0.1.0'\nedition='2024'\n{}[workspace]\n",
            if macros {
                "[dependencies]\nmacros = { path = 'macros' }\n"
            } else {
                ""
            }
        ),
    )
    .into_diagnostic()?;
    if macros {
        std::fs::create_dir_all(root.path().join("macros/src")).into_diagnostic()?;
        std::fs::write(
            root.path().join("macros/Cargo.toml"),
            "[package]\nname='macros'\nversion='0.1.0'\nedition='2024'\n[lib]\nproc-macro=true\n",
        )
        .into_diagnostic()?;
        std::fs::write(root.path().join("macros/src/lib.rs"), MACROS).into_diagnostic()?;
    }
    if tests {
        // An integration test binary: a leaf of the crate's graph, read under the `test` cfg and
        // under the contexts its own gates ask for, after the library's contexts.
        std::fs::create_dir_all(root.path().join("tests")).into_diagnostic()?;
        let mut text = String::from(
            "use grown::m0::helper0;\n#[test]\nfn calls_the_library() { let _ = helper0(); }\n",
        );
        for context in 0..contexts.min(3) {
            text.push_str(&format!(
                "#[cfg(c{context})]\n#[test]\nfn gated_{context}() {{ let _ = helper0(); }}\n"
            ));
        }
        std::fs::write(root.path().join("tests/leaf.rs"), text).into_diagnostic()?;
    }
    if member {
        // A package that builds on the first, with gates of its own and a call into it.
        std::fs::create_dir_all(root.path().join("extra/src")).into_diagnostic()?;
        std::fs::write(
            root.path().join("extra/Cargo.toml"),
            "[package]\nname='extra'\nversion='0.1.0'\nedition='2024'\n\
             [dependencies]\ngrown = { path = '..' }\n",
        )
        .into_diagnostic()?;
        let mut text = String::from("pub fn through() { let _ = grown::m1::helper1(); }\n");
        for context in 0..contexts.min(3) {
            text.push_str(&format!(
                "#[cfg(c{context})]\npub fn extra_{context}() {{ through(); }}\n"
            ));
        }
        std::fs::write(root.path().join("extra/src/lib.rs"), text).into_diagnostic()?;
    }
    let mods: String = (0..FILES).map(|i| format!("pub mod m{i};\n")).collect();
    std::fs::write(root.path().join("src/lib.rs"), mods).into_diagnostic()?;
    for index in 0..FILES {
        std::fs::write(
            root.path().join(format!("src/m{index}.rs")),
            module(index, contexts, macros),
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
    verification_seen(contexts, Setup::default(), &Writes::default())
}

fn verification_seen(contexts: usize, setup: Setup, writes: &Writes) -> Result<Verified> {
    let root = fixture(contexts, setup)?;
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
                    .with_target("salsa::stale", tracing::Level::DEBUG)
                    .with_target("cordial::milestones", tracing::Level::DEBUG)
                    .with_target("salsa::explain", tracing::Level::DEBUG)
                    .with_target("salsa::function::execute", tracing::Level::INFO),
            ),
        );
    // Every file of the package, as a run projects them.
    let mut paths = vec![root.path().join("src/lib.rs")];
    paths.extend((0..FILES).map(|index| root.path().join(format!("src/m{index}.rs"))));
    tracing::subscriber::with_default(subscriber, || -> Result<()> {
        let mut session =
            SourceDatabase::load_saved(root.path(), &options, Some(saved)).into_diagnostic()?;
        session
            .project(&CrateTarget::new("grown", root.path()), &paths)
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
    let verified = verification_seen(CONTEXTS, Setup::default(), &writes)?;
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

/// The same with a procedural macro in the crates: what a derive expands to, and what is built
/// from that, must not run again at the save either.
#[test]
fn verifying_after_several_contexts_with_a_proc_macro_runs_nothing_again() -> Result<()> {
    cordial::init_tracing();
    let writes = Writes::default();
    let verified = verification_seen(
        CONTEXTS,
        Setup {
            macros: true,
            ..Setup::default()
        },
        &writes,
    )?;
    assert_eq!(verified.analysed, CONTEXTS as u64);
    assert!(
        writes
            .0
            .lock()
            .expect("a lock")
            .keys()
            .any(|name| name.starts_with("exec expand_proc_macro")),
        "a procedural macro was expanded, or this audits nothing"
    );
    assert!(verified.memos > 0, "something was there to verify");
    assert_eq!(
        verified.queries,
        0,
        "verifying ran these again: [{}]; out of date: {:?}",
        verified.top,
        writes.0.lock().expect("a lock")
    );
    Ok(())
}

/// The same with an integration test binary in the package: the leaf pass enters contexts of its
/// own after the library's, and the save verifies after all of them.
#[test]
fn verifying_after_the_leaf_pass_runs_nothing_again() -> Result<()> {
    cordial::init_tracing();
    let writes = Writes::default();
    let verified = verification_seen(
        CONTEXTS,
        Setup {
            tests: true,
            ..Setup::default()
        },
        &writes,
    )?;
    if verified.queries > 0 {
        // Where the writes the memos went out of date for came in the run.
        let order = writes.1.lock().expect("a lock");
        let mut shown: Vec<String> = Vec::new();
        for event in order.iter() {
            let interesting = event.starts_with("explain")
                || event.starts_with("--")
                || event.starts_with("stale")
                || (event.starts_with("write")
                    && !event.contains("SourceRootInput")
                    && !event.contains("ToolRoots"));
            if interesting && shown.last() != Some(event) {
                shown.push(event.clone());
            }
        }
        println!("order: {shown:#?}");
    }
    assert!(verified.memos > 0, "something was there to verify");
    assert_eq!(
        verified.queries,
        0,
        "verifying ran these again: [{}]; out of date: {:?}",
        verified.top,
        writes.0.lock().expect("a lock")
    );
    Ok(())
}

/// Everything at once: a derive, an integration test binary and a second package that builds on the
/// first, which is what a real workspace has.
#[test]
fn verifying_a_workspace_with_everything_runs_nothing_again() -> Result<()> {
    cordial::init_tracing();
    let writes = Writes::default();
    let verified = verification_seen(
        CONTEXTS,
        Setup {
            macros: true,
            tests: true,
            member: true,
        },
        &writes,
    )?;
    assert!(
        writes
            .0
            .lock()
            .expect("a lock")
            .keys()
            .any(|name| name.starts_with("exec expand_proc_macro")),
        "a procedural macro was expanded, or this audits nothing"
    );
    assert!(verified.memos > 0, "something was there to verify");
    assert_eq!(
        verified.queries,
        0,
        "verifying ran these again: [{}]; out of date: {:?}",
        verified.top,
        writes.0.lock().expect("a lock")
    );
    Ok(())
}

//! A run after an edit reuses what the edit did not reach, and what it reuses is what a run from
//! nothing would have computed.
//!
//! Evidence about a function is saved as depending on a fingerprint of its crate's definition map,
//! not on the text of every file the map is made from. That is only sound if the fingerprint
//! changes whenever anything a function can read of the map does, and the way to know is to
//! compare. For each kind of edit, a run restored from the state saved before the edit must give
//! what a run from nothing gives on the edited sources, and the edits that must change what is
//! said must change it. An edit a fingerprint missed would leave the first run saying what it said
//! before.

mod support {
    pub mod executions;
}

use cordial::{
    BasicQuery, CfgPlanning, CrateIr, CrateTarget, IrView, SavedState, SemanticCargoOptions,
    SourceDatabase,
};
use miette::{IntoDiagnostic, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use support::executions::Executions;
use tracing_subscriber::prelude::*;

/// What is written under `src/`: file name and text.
type Sources = Vec<(&'static str, String)>;

/// A crate of four files that call into one another, a macro that defines a function, and a type
/// with a method, so that each kind of edit has something to reach.
fn original() -> Sources {
    vec![
        ("lib.rs", "#[macro_use]\nmod macros;\npub mod a;\npub mod b;\npub mod c;\n".into()),
        (
            "macros.rs",
            "macro_rules! make {\n    ($n:ident) => {\n        pub fn $n() -> u8 {\n            1\n        }\n    };\n}\n\
             macro_rules! inner {\n    ($n:ident) => {\n        pub fn $n() -> u8 {\n            3\n        }\n    };\n}\n\
             macro_rules! outer {\n    ($n:ident) => {\n        inner!($n);\n    };\n}\n"
                .into(),
        ),
        (
            "a.rs",
            "make!(made);\n\
             outer!(nested);\n\
             pub fn a_leaf() {}\n\
             pub fn a_top() {\n    a_leaf();\n}\n\
             pub struct A;\n\
             impl A {\n    pub fn m(&self) {}\n}\n\
             pub struct A2;\n\
             pub trait T {\n    fn t(&self) -> u8;\n}\n\
             impl T for A {\n    fn t(&self) -> u8 {\n        1\n    }\n}\n"
                .into(),
        ),
        (
            "b.rs",
            "pub fn b_top() {\n    crate::a::a_top();\n}\n\
             pub fn b_leaf() {}\n"
                .into(),
        ),
        (
            "c.rs",
            "use crate::a::T;\npub fn c_top() {\n    crate::b::b_top();\n    crate::b::b_leaf();\n    crate::a::a_leaf();\n    crate::a::made();\n    crate::a::nested();\n    let a = crate::a::A;\n    a.m();\n    a.t();\n}\n"
                .into(),
        ),
    ]
}

/// What an edit changes, and whether it must change what is said about the crate.
struct Edit {
    name: &'static str,
    apply: fn(&mut Sources),
    /// Whether what a run says about the crate is different after the edit.
    changes_what_is_said: bool,
}

fn replace(sources: &mut Sources, file: &str, from: &str, to: &str) {
    let entry = sources
        .iter_mut()
        .find(|(name, _)| *name == file)
        .unwrap_or_else(|| panic!("{file} is in the specimen"));
    assert!(entry.1.contains(from), "{file} has {from:?} to replace");
    entry.1 = entry.1.replacen(from, to, 1);
}

fn edits() -> Vec<Edit> {
    vec![
        Edit {
            name: "a comment above everything",
            apply: |s| replace(s, "a.rs", "make!(made);", "// a comment\nmake!(made);"),
            changes_what_is_said: false,
        },
        Edit {
            name: "the body of a function nothing in another file calls",
            apply: |s| {
                replace(
                    s,
                    "b.rs",
                    "pub fn b_leaf() {}",
                    "pub fn b_leaf() {\n    let _ = 1;\n}",
                )
            },
            changes_what_is_said: false,
        },
        Edit {
            name: "the return type of a function others call",
            apply: |s| {
                replace(
                    s,
                    "a.rs",
                    "pub fn a_leaf() {}",
                    "pub fn a_leaf() -> u8 {\n    1\n}",
                )
            },
            changes_what_is_said: true,
        },
        Edit {
            name: "a function moved to another file and re-exported",
            apply: |s| {
                replace(s, "b.rs", "pub fn b_leaf() {}", "pub use crate::c::b_leaf;");
                replace(
                    s,
                    "c.rs",
                    "pub fn c_top() {",
                    "pub fn b_leaf() {}\npub fn c_top() {",
                );
            },
            changes_what_is_said: true,
        },
        Edit {
            name: "a function made private that another file calls",
            apply: |s| replace(s, "a.rs", "pub fn a_leaf() {}", "fn a_leaf() {}"),
            changes_what_is_said: true,
        },
        Edit {
            name: "a function renamed",
            apply: |s| {
                replace(s, "a.rs", "pub fn a_leaf() {}", "pub fn a_leaf2() {}");
                replace(s, "a.rs", "    a_leaf();", "    a_leaf2();");
            },
            changes_what_is_said: true,
        },
        Edit {
            name: "what a macro defines",
            apply: |s| {
                replace(
                    s,
                    "macros.rs",
                    "pub fn $n() -> u8 {\n            1",
                    "pub fn $n() -> u16 {\n            1",
                )
            },
            changes_what_is_said: true,
        },
        Edit {
            name: "what a macro that another macro calls defines",
            apply: |s| {
                replace(
                    s,
                    "macros.rs",
                    "pub fn $n() -> u8 {\n            3",
                    "pub fn $n() -> u32 {\n            3",
                )
            },
            changes_what_is_said: true,
        },
        Edit {
            name: "the return type of a method",
            apply: |s| {
                replace(
                    s,
                    "a.rs",
                    "pub fn m(&self) {}",
                    "pub fn m(&self) -> u8 {\n        1\n    }",
                )
            },
            changes_what_is_said: true,
        },
        Edit {
            name: "a trait impl moved to another type",
            apply: |s| replace(s, "a.rs", "impl T for A {", "impl T for A2 {"),
            changes_what_is_said: true,
        },
        Edit {
            name: "an inherent impl moved to another type",
            apply: |s| replace(s, "a.rs", "impl A {", "impl A2 {"),
            changes_what_is_said: true,
        },
        Edit {
            name: "a trait impl added for another type",
            apply: |s| {
                replace(
                    s,
                    "a.rs",
                    "pub struct A2;",
                    "pub struct A2;\nimpl T for A2 {\n    fn t(&self) -> u8 {\n        a_leaf();\n        2\n    }\n}",
                )
            },
            changes_what_is_said: false,
        },
        Edit {
            name: "the body of a trait method",
            apply: |s| {
                replace(
                    s,
                    "a.rs",
                    "fn t(&self) -> u8 {\n        1",
                    "fn t(&self) -> u8 {\n        let _ = 3;\n        1",
                )
            },
            changes_what_is_said: false,
        },
        Edit {
            name: "a function gated off",
            apply: |s| {
                replace(
                    s,
                    "a.rs",
                    "pub fn a_leaf() {}",
                    "#[cfg(any())]\npub fn a_leaf() {}",
                )
            },
            changes_what_is_said: true,
        },
    ]
}

fn write(root: &Path, sources: &Sources) -> Result<()> {
    std::fs::create_dir_all(root.join("src")).into_diagnostic()?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='gated'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    for (name, text) in sources {
        std::fs::write(root.join("src").join(name), text).into_diagnostic()?;
    }
    Ok(())
}

fn sources_of(root: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(root.join("src"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .collect();
    found.sort();
    found
}

/// Whether the reuse across edits is on, as a run is asked.
#[derive(Clone, Copy, Debug)]
enum Reuse {
    On,
    Off,
}

/// What the audit of a run said, from its events: how many reused functions it computed again, and
/// how many of them were not what the sources give.
#[derive(Default, Clone)]
struct AuditEvents(Arc<Mutex<Vec<(u64, u64)>>>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for AuditEvents {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        if event.metadata().target() != "cordial::audit" {
            return;
        }
        #[derive(Default)]
        struct Fields {
            audited: u64,
            mismatched: u64,
            summary: bool,
        }
        impl tracing::field::Visit for Fields {
            fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
                match field.name() {
                    "audited" => {
                        self.audited = value;
                        self.summary = true;
                    }
                    "mismatched" => self.mismatched = value,
                    _ => {}
                }
            }
            fn record_debug(&mut self, _: &tracing::field::Field, _: &dyn std::fmt::Debug) {}
        }
        let mut fields = Fields::default();
        event.record(&mut fields);
        if fields.summary
            && let Ok(mut all) = self.0.lock()
        {
            all.push((fields.audited, fields.mismatched));
        }
    }
}

/// What a run says: all of it, with ids taken out (they are numbered as the run met them), and the
/// facts about each function with where things are taken out too, so that an edit that only moves
/// things is not taken for one that changes what they say.
struct Said {
    all: serde_json::Value,
    facts: BTreeMap<String, serde_json::Value>,
    /// Reused functions the run computed again, and how many of those differed.
    audited: u64,
    mismatched: u64,
}

/// The facts about each function by name, without ids or places.
fn facts_of(graph: &CrateIr) -> Result<BTreeMap<String, serde_json::Value>> {
    fn without_places(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                map.retain(|key, _| {
                    ![
                        "range", "start", "end", "line", "column", "offset", "digest",
                    ]
                    .iter()
                    .any(|place| key.contains(place))
                });
                map.values_mut().for_each(without_places);
            }
            serde_json::Value::Array(items) => items.iter_mut().for_each(without_places),
            _ => {}
        }
    }
    let mut found = BTreeMap::new();
    for node in graph.nodes_matching(&BasicQuery::items()) {
        let Some(name) = node.weight().name().clone() else {
            continue;
        };
        let Some(facts) = graph.semantic_function(node.id()).into_diagnostic()? else {
            continue;
        };
        let mut value = serde_json::to_value(facts).into_diagnostic()?;
        without_ids(&mut value);
        without_places(&mut value);
        found.insert(name, value);
    }
    Ok(found)
}

/// What a run says about the crate, and how many functions' evidence it computed.
fn run(root: &Path, saved: &SavedState, reuse: Reuse) -> Result<(Said, usize)> {
    let (said, computed, _) = run_with(root, saved, reuse, CfgPlanning::Explicit)?;
    Ok((said, computed))
}

/// A run, with how many functions' evidence it computed and how many queries of any kind it
/// executed.
fn run_with(
    root: &Path,
    saved: &SavedState,
    reuse: Reuse,
    planning: CfgPlanning,
) -> Result<(Said, usize, usize)> {
    let executions = Executions::default();
    let audit = AuditEvents::default();
    let subscriber = tracing_subscriber::registry()
        .with(executions.clone())
        .with(audit.clone())
        .with(cordial::tracing_fmt_layer());
    let _subscriber = tracing::subscriber::set_default(subscriber);
    let options = SemanticCargoOptions::default()
        .with_cfg_planning(planning)
        .with_reuse_across_edits(matches!(reuse, Reuse::On));
    let mut session =
        SourceDatabase::load_saved(root, &options, Some(saved.clone())).into_diagnostic()?;
    let graph = session
        .project(&CrateTarget::new("gated", root), &sources_of(root))
        .into_diagnostic()?;
    let mut all = serde_json::to_value(graph.snapshot().into_diagnostic()?).into_diagnostic()?;
    without_ids(&mut all);
    let (audited, mismatched) = audit
        .0
        .lock()
        .map(|all| all.iter().fold((0, 0), |(a, m), (x, y)| (a + x, m + y)))
        .unwrap_or_default();
    let said = Said {
        all,
        facts: facts_of(&graph)?,
        audited,
        mismatched,
    };
    let all = executions.take();
    let computed = all
        .iter()
        .filter(|(name, _)| name.ends_with("> persisted_evidence"))
        .map(|(_, count)| count)
        .sum();
    let executed = all.values().sum();
    // Which kinds of query ran the most, for a failure to say what it was that ran.
    let mut most: Vec<_> = all.iter().collect();
    most.sort_by_key(|(name, count)| (std::cmp::Reverse(**count), (*name).clone()));
    *RAN_MOST
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = most
        .into_iter()
        .take(40)
        .map(|(name, count)| format!("{name}={count}"))
        .collect::<Vec<_>>()
        .join(" ");
    Ok((said, computed, executed))
}

/// The kinds of query the last run executed the most of.
static RAN_MOST: Mutex<String> = Mutex::new(String::new());

fn without_ids(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            map.remove("definition");
            if let Some(serde_json::Value::Object(target)) = map.get_mut("target")
                && let Some(id) = target.get_mut("Function")
            {
                *id = serde_json::Value::from(0);
            }
            map.values_mut().for_each(without_ids);
        }
        serde_json::Value::Array(items) => items.iter_mut().for_each(without_ids),
        _ => {}
    }
}

/// What a run says about the edited sources from nothing, from the state a run on the original
/// sources saved, and how many functions each computed.
struct Outcome {
    before: Said,
    cold: Said,
    restored: Said,
}

fn after(edit: &Edit, reuse: Reuse) -> Result<Outcome> {
    cordial::init_tracing();
    let root = tempfile::tempdir().into_diagnostic()?;
    write(root.path(), &original())?;
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = SavedState::at(state.path().join("analysis-state"));
    let (before, _) = run(root.path(), &saved, reuse)?;
    let mut sources = original();
    (edit.apply)(&mut sources);
    write(root.path(), &sources)?;
    let (restored, _) = run(root.path(), &saved, reuse)?;
    let nothing = tempfile::tempdir().into_diagnostic()?;
    let (cold, _) = run(
        root.path(),
        &SavedState::at(nothing.path().join("analysis-state")),
        reuse,
    )?;
    Ok(Outcome {
        before,
        cold,
        restored,
    })
}

/// Where two values first differ, as the path to it and the two things there, so that a failure
/// says what is different instead of printing both.
fn first_difference(left: &serde_json::Value, right: &serde_json::Value) -> String {
    fn walk(
        path: &mut Vec<String>,
        left: &serde_json::Value,
        right: &serde_json::Value,
    ) -> Option<String> {
        use serde_json::Value::{Array, Object};
        match (left, right) {
            (Object(a), Object(b)) => {
                for key in a.keys().chain(b.keys()) {
                    path.push(key.clone());
                    let found = match (a.get(key), b.get(key)) {
                        (Some(x), Some(y)) => walk(path, x, y),
                        (x, y) => Some(format!("{}: {x:?} against {y:?}", path.join("/"))),
                    };
                    path.pop();
                    if found.is_some() {
                        return found;
                    }
                }
                None
            }
            (Array(a), Array(b))
                if a != b
                    && a.iter()
                        .chain(b)
                        .all(|item| !item.is_object() && !item.is_array()) =>
            {
                Some(format!("{}: {a:?} against {b:?}", path.join("/")))
            }
            (Array(a), Array(b)) if a.len() == b.len() => {
                for (index, (x, y)) in a.iter().zip(b).enumerate() {
                    path.push(index.to_string());
                    let found = walk(path, x, y);
                    path.pop();
                    if found.is_some() {
                        return found;
                    }
                }
                None
            }
            (a, b) if a == b => None,
            (a, b) => Some(format!("{}: {a} against {b}", path.join("/"))),
        }
    }
    walk(&mut Vec::new(), left, right).unwrap_or_else(|| "nothing".into())
}

/// A run restored from before `edit` says what a run from nothing says after it, and the edit
/// changes the facts when it should.
fn check(index: usize, reuse: Reuse) -> Result<()> {
    let edit = &edits()[index];
    let outcome = after(edit, reuse)?;
    assert!(
        outcome.restored.all == outcome.cold.all,
        "{reuse:?}, {}: a run restored from before the edit said something else than a run from \
         nothing; the first difference, restored against cold, {}",
        edit.name,
        first_difference(&outcome.restored.all, &outcome.cold.all)
    );
    assert_eq!(
        outcome.restored.facts, outcome.cold.facts,
        "{reuse:?}, {}: the facts a restored run gave are not the ones a run from nothing gave",
        edit.name
    );
    assert_eq!(
        outcome.restored.mismatched, 0,
        "{reuse:?}, {}: the audit found {} of {} reused functions not what the sources give",
        edit.name, outcome.restored.mismatched, outcome.restored.audited
    );
    assert_eq!(
        outcome.cold.facts != outcome.before.facts,
        edit.changes_what_is_said,
        "{reuse:?}, {}: the facts {} by the edit",
        edit.name,
        if edit.changes_what_is_said {
            "should change"
        } else {
            "should not move"
        }
    );
    Ok(())
}

/// A test for each kind of edit with reuse across edits on, so that they run side by side.
macro_rules! after_an_edit {
    ($($name:ident => $index:expr),* $(,)?) => {
        $(
            #[test]
            fn $name() -> Result<()> {
                check($index, Reuse::On)
            }
        )*
    };
}

after_an_edit! {
    a_comment_above_everything => 0,
    the_body_of_a_function_nothing_in_another_file_calls => 1,
    the_return_type_of_a_function_others_call => 2,
    a_function_moved_to_another_file_and_re_exported => 3,
    a_function_made_private_that_another_file_calls => 4,
    a_function_renamed => 5,
    what_a_macro_defines => 6,
    what_a_macro_that_another_macro_calls_defines => 7,
    the_return_type_of_a_method => 8,
    a_trait_impl_moved_to_another_type => 9,
    an_inherent_impl_moved_to_another_type => 10,
    a_trait_impl_added_for_another_type => 11,
    the_body_of_a_trait_method => 12,
    a_function_gated_off => 13,
}

/// The same checks with reuse off, which is the harness telling the two edits that matter most
/// apart from nothing: if these passed with reuse on and failed here, the harness would be wrong.
#[test]
fn the_checks_hold_without_reuse_too() -> Result<()> {
    check(2, Reuse::Off)?;
    check(3, Reuse::Off)?;
    check(11, Reuse::Off)
}

/// A crate of `FILES` modules of `PER_FILE` functions, each calling the one before it in its own
/// file: nothing calls into another file, and the last of each file has no caller.
const FILES: usize = 6;
const PER_FILE: usize = 5;

fn independent_files() -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    let mut sources: Sources = Vec::new();
    let mut library = String::new();
    for file in 0..FILES {
        library.push_str(&format!("pub mod file{file};\n"));
        let mut text = String::new();
        for function in 0..PER_FILE {
            let call = match function {
                0 => String::new(),
                n => format!("    f{file}_{}();\n", n - 1),
            };
            text.push_str(&format!("pub fn f{file}_{function}() {{\n{call}}}\n"));
        }
        sources.push((Box::leak(format!("file{file}.rs").into_boxed_str()), text));
    }
    sources.push(("lib.rs", library));
    write(root.path(), &sources)?;
    Ok(root)
}

/// How many functions' evidence a restored run computed after the body of the last function of
/// the third file changed, which nothing calls.
fn computed_after_a_body_edit(reuse: Reuse) -> Result<(usize, u64, u64)> {
    cordial::init_tracing();
    let root = independent_files()?;
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = SavedState::at(state.path().join("analysis-state"));
    run(root.path(), &saved, reuse)?;
    let edited = root.path().join("src/file2.rs");
    let text = std::fs::read_to_string(&edited).into_diagnostic()?;
    std::fs::write(
        &edited,
        text.replace(
            &format!("pub fn f2_{}() {{", PER_FILE - 1),
            &format!("pub fn f2_{}() {{\n    let _ = 1;", PER_FILE - 1),
        ),
    )
    .into_diagnostic()?;
    let (said, computed) = run(root.path(), &saved, reuse)?;
    Ok((computed, said.audited, said.mismatched))
}

/// With reuse across edits, an edit to a function's body makes a run compute the functions of the
/// file it is in and not those of the crate: the definition map the edit makes is the one there was,
/// and nothing else reads the file. (One more is the warm-up's own query, which every restored run
/// computes.) Without it, every function of the crate is computed.
#[test]
fn an_edit_to_a_body_computes_the_functions_of_its_file_and_not_the_crate() -> Result<()> {
    let (everything, audited_without, _) = computed_after_a_body_edit(Reuse::Off)?;
    let (reused, audited, mismatched) = computed_after_a_body_edit(Reuse::On)?;
    assert_eq!(
        audited_without, 0,
        "there is no audit without reuse to check"
    );
    assert!(
        audited > 0,
        "the audit computed again some of what was reused"
    );
    assert_eq!(
        mismatched, 0,
        "none of what was reused differs from what the sources give"
    );
    assert!(
        everything > FILES * PER_FILE,
        "without reuse a run computed {everything} of {} functions",
        FILES * PER_FILE
    );
    assert!(
        reused <= PER_FILE + 1,
        "with reuse a run computed {reused}: more than the {PER_FILE} functions of the file and \
         the warm-up's query"
    );
    Ok(())
}

/// How many definition maps a run found to have things in them with no meaning that survives the
/// process, from the events it says so in.
#[derive(Default, Clone)]
struct VolatileMaps(Arc<Mutex<Vec<u64>>>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for VolatileMaps {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        struct Count(Option<u64>);
        impl tracing::field::Visit for Count {
            fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
                if field.name() == "volatile" {
                    self.0 = Some(value);
                }
            }
            fn record_debug(&mut self, _: &tracing::field::Field, _: &dyn std::fmt::Debug) {}
        }
        let mut count = Count(None);
        event.record(&mut count);
        if let Some(volatile) = count.0
            && event.metadata().target() == "cordial::cache"
            && let Ok(mut all) = self.0.lock()
        {
            all.push(volatile);
        }
    }
}

/// Everything in the specimen's definition maps, including what macros that call macros make, has a
/// meaning that survives the process, so nothing about the crate is volatile and an edit that
/// leaves the map as it was reuses what read it.
#[test]
fn the_specimens_definition_maps_mean_the_same_in_every_process() -> Result<()> {
    cordial::init_tracing();
    let volatile = VolatileMaps::default();
    let subscriber = tracing_subscriber::registry().with(volatile.clone());
    let _subscriber = tracing::subscriber::set_default(subscriber);
    let root = tempfile::tempdir().into_diagnostic()?;
    write(root.path(), &original())?;
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = SavedState::at(state.path().join("analysis-state"));
    run(root.path(), &saved, Reuse::On)?;
    let found = volatile.0.lock().map(|all| all.clone()).unwrap_or_default();
    assert!(
        found.is_empty(),
        "definition maps with volatile things in them: {found:?}"
    );
    Ok(())
}

/// A crate like the independent files, with what cordial has that they do not: each file has a type
/// with an inherent method, a trait impl, and functions that call both.
fn independent_files_with_impls() -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    let mut sources: Sources = Vec::new();
    let mut library = String::new();
    for file in 0..FILES {
        library.push_str(&format!("pub mod file{file};\n"));
        let mut text = format!(
            "pub struct S{file};\nimpl S{file} {{\n    pub fn m(&self) {{}}\n}}\nimpl Default for S{file} {{\n    fn default() -> Self {{\n        S{file}\n    }}\n}}\n"
        );
        for function in 0..PER_FILE {
            text.push_str(&format!(
                "pub fn f{file}_{function}() {{\n    let s = S{file}::default();\n    s.m();\n}}\n"
            ));
        }
        sources.push((Box::leak(format!("file{file}.rs").into_boxed_str()), text));
    }
    sources.push(("lib.rs", library));
    write(root.path(), &sources)?;
    Ok(root)
}

/// How many functions' evidence a restored run computed after the body of the last function of the
/// third file changed, in a crate whose files each have an inherent method and a trait impl.
fn computed_after_a_body_edit_with_impls() -> Result<(usize, u64, u64)> {
    cordial::init_tracing();
    let root = independent_files_with_impls()?;
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = SavedState::at(state.path().join("analysis-state"));
    run(root.path(), &saved, Reuse::On)?;
    let edited = root.path().join("src/file2.rs");
    let text = std::fs::read_to_string(&edited).into_diagnostic()?;
    std::fs::write(
        &edited,
        text.replace(
            &format!("pub fn f2_{}() {{", PER_FILE - 1),
            &format!("pub fn f2_{}() {{\n    let _ = 1;", PER_FILE - 1),
        ),
    )
    .into_diagnostic()?;
    let (said, computed) = run(root.path(), &saved, Reuse::On)?;
    Ok((computed, said.audited, said.mismatched))
}

/// With impls in the crate, as cordial has, an edit to a function's body still computes the
/// functions of its file and not the crate's. (When it does not, the flatten report names the kinds
/// of query that read the most inputs themselves: `salsa::flatten_breadth`, and in a log
/// `RUST_LOG=salsa::persistence=info`.)
#[test]
fn an_edit_to_a_body_computes_the_functions_of_its_file_when_the_crate_has_impls() -> Result<()> {
    let (computed, audited, mismatched) = computed_after_a_body_edit_with_impls()?;
    assert_eq!(
        mismatched, 0,
        "none of what was reused differs from what the sources give"
    );
    // Each file has PER_FILE functions, an inherent method and a trait method.
    let file = PER_FILE + 2;
    assert!(
        computed <= file + 1,
        "{computed} of {} functions were computed after an edit to one body in one file ({audited} \
         of what was reused was audited); what reads the most inputs itself: {}",
        FILES * file,
        salsa::flatten_breadth("persisted_evidence").unwrap_or_default()
    );
    Ok(())
}

/// A crate like the independent files, with a function in each file that a Cargo feature gates, so
/// that the planner adds a cfg context in which those files are compiled again, as cordial's own
/// features do.
fn independent_files_with_a_gated_function() -> Result<tempfile::TempDir> {
    let root = independent_files()?;
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='gated'\nversion='0.1.0'\nedition='2024'\n[features]\nextra=[]\n[workspace]\n",
    )
    .into_diagnostic()?;
    for file in 0..FILES {
        let path = root.path().join(format!("src/file{file}.rs"));
        let mut text = std::fs::read_to_string(&path).into_diagnostic()?;
        text.push_str(&format!(
            "#[cfg(feature = \"extra\")]\npub fn gated{file}() {{\n    f{file}_0();\n}}\n"
        ));
        std::fs::write(&path, text).into_diagnostic()?;
    }
    Ok(root)
}

/// What a restored run does after an edit to one body, in a crate with a cfg context, against what
/// the run from nothing did: the functions' evidence computed and the queries executed by each.
fn after_a_body_edit_with_a_context() -> Result<((usize, usize), (usize, usize))> {
    cordial::init_tracing();
    let root = independent_files_with_a_gated_function()?;
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = SavedState::at(state.path().join("analysis-state"));
    let (_, cold_computed, cold_executed) =
        run_with(root.path(), &saved, Reuse::On, CfgPlanning::Automatic)?;
    let edited = root.path().join("src/file2.rs");
    let text = std::fs::read_to_string(&edited).into_diagnostic()?;
    std::fs::write(
        &edited,
        text.replace(
            &format!("pub fn f2_{}() {{", PER_FILE - 1),
            &format!("pub fn f2_{}() {{\n    let _ = 1;", PER_FILE - 1),
        ),
    )
    .into_diagnostic()?;
    let (_, computed, executed) = run_with(root.path(), &saved, Reuse::On, CfgPlanning::Automatic)?;
    Ok(((cold_computed, cold_executed), (computed, executed)))
}

/// With a cfg context (as cordial has dozens of), an edit to one body makes a restored run execute
/// a small part of what the run from nothing did, and compute the evidence of the functions of one
/// file in the ordinary build and in the context, not of the crate.
#[test]
fn an_edit_to_a_body_costs_a_small_part_of_a_cold_run_when_the_crate_has_a_cfg_context()
-> Result<()> {
    let ((cold_computed, cold_executed), (computed, executed)) =
        after_a_body_edit_with_a_context()?;
    tracing::info!(
        target: "edit_reuse",
        cold_executed,
        cold_computed,
        executed,
        computed,
        most = %RAN_MOST.lock().map(|most| most.clone()).unwrap_or_default(),
        "what the run from nothing and the run restored after an edit did"
    );
    assert!(
        executed * 4 < cold_executed,
        "the edited run executed {executed} queries, {computed} of them functions' evidence; the run \
         from nothing executed {cold_executed}, {cold_computed} of them functions' evidence; the \
         most of the edited run: {}",
        RAN_MOST.lock().map(|most| most.clone()).unwrap_or_default()
    );
    // Each file's functions, once in the ordinary build and once in the context.
    assert!(
        computed <= 2 * (PER_FILE + 1) + 1,
        "the edited run computed the evidence of {computed} functions, against {cold_computed} from \
         nothing"
    );
    Ok(())
}

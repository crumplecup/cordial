//! A projection keeps the syntax tree of every file it parses unless it is asked to evict, and with
//! the option it keeps a bounded number.
//!
//! The theory under test: Salsa drops the values past the capacity of a query that keeps the most
//! recently used (128 files, 512 expansions) only when asked to, with the database held
//! exclusively, and cordial asks nowhere unless told to. So a run that parses many more files than
//! the capacity holds all their trees when the priming ends; and when the priming evicts between its
//! chunks, and waits for the dropped trees to be freed, it holds the capacity's worth.
//!
//! The crate has 220 files, each a large literal, so that a file's tree is about a megabyte.

#[path = "../src/counting_allocator.rs"]
mod counting_allocator;

use cordial::{CfgPlanning, CrateTarget, SavedState, SemanticCargoOptions, SourceDatabase};
use miette::{IntoDiagnostic, Result};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::SubscriberExt;

#[global_allocator]
static ALLOCATOR: counting_allocator::Counting = counting_allocator::Counting;

const FILES: usize = 220;
const ELEMENTS: usize = 2500;

/// The bytes the events of `cordial::heap` say, by their label, and what the evictions gave back.
#[derive(Default)]
struct Said {
    held: Vec<(String, u64)>,
    evictions: usize,
    given_back: u64,
}

struct Capture(Arc<Mutex<Said>>);

impl<S: tracing::Subscriber> Layer<S> for Capture {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        #[derive(Default)]
        struct Fields {
            label: String,
            bytes: u64,
            settled: u64,
        }
        impl tracing::field::Visit for Fields {
            fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
                match field.name() {
                    "bytes" => self.bytes = value,
                    "freed_settled_bytes" => self.settled = value,
                    _ => {}
                }
            }
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                if field.name() == "ingredient" {
                    self.label = format!("{value:?}").trim_matches('"').to_owned();
                }
            }
        }
        let target = event.metadata().target();
        if target != "cordial::heap" && target != "cordial::eviction" {
            return;
        }
        let mut fields = Fields::default();
        event.record(&mut fields);
        if let Ok(mut said) = self.0.lock() {
            if target == "cordial::eviction" {
                said.evictions += 1;
                said.given_back += fields.settled;
            } else {
                said.held.push((fields.label, fields.bytes));
            }
        }
    }
}

fn crate_of_large_files() -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir(root.path().join("src")).into_diagnostic()?;
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname = \"chunked\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .into_diagnostic()?;
    let table = (0..ELEMENTS)
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let mut library = String::new();
    for file in 0..FILES {
        library.push_str(&format!("pub mod file{file};\n"));
        std::fs::write(
            root.path().join(format!("src/file{file}.rs")),
            format!(
                "pub static TABLE{file}: [u32; {ELEMENTS}] = [{table}];\npub fn f{file}() -> u32 {{ TABLE{file}[1] }}\n"
            ),
        )
        .into_diagnostic()?;
    }
    std::fs::write(root.path().join("src/lib.rs"), library).into_diagnostic()?;
    Ok(root)
}

fn sources_of(root: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(root.join("src"))
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .collect();
    found.sort();
    found
}

/// Wait for the memory held to stop falling: what a dropped session hands to the thread that frees
/// syntax trees is not freed when it is dropped.
fn settle() {
    let mut last = counting_allocator::held_in_all();
    let mut steady = 0;
    for _ in 0..500 {
        std::thread::sleep(std::time::Duration::from_millis(20));
        let now = counting_allocator::held_in_all();
        steady = if now < last { 0 } else { steady + 1 };
        last = now;
        if steady >= 10 {
            break;
        }
    }
}

/// A projection of the crate, with or without evicting, and what its events said.
fn project(evicts: bool) -> Result<Said> {
    let root = crate_of_large_files()?;
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = SavedState::at(state.path().join("analysis-state"));
    let said = Arc::new(Mutex::new(Said::default()));
    let subscriber = tracing_subscriber::registry().with(
        Capture(said.clone()).with_filter(
            tracing_subscriber::filter::Targets::new()
                .with_target("cordial::heap", tracing::Level::INFO)
                .with_target("cordial::eviction", tracing::Level::INFO),
        ),
    );
    let _subscriber = tracing::subscriber::set_default(subscriber);
    let options = SemanticCargoOptions::default()
        .with_cfg_planning(CfgPlanning::Explicit)
        .with_evicts_between_chunks(evicts);
    let mut session =
        SourceDatabase::load_saved(root.path(), &options, Some(saved)).into_diagnostic()?;
    session
        .project(
            &CrateTarget::new("chunked", root.path()),
            &sources_of(root.path()),
        )
        .into_diagnostic()?;
    drop(session);
    let said = std::mem::take(&mut *said.lock().expect("the events"));
    Ok(said)
}

/// What the trees held when the priming ended (the stage called `evidence`).
fn trees_at_the_end_of_priming(said: &Said) -> u64 {
    said.held
        .iter()
        .filter(|(label, _)| {
            label.starts_with("evidence ")
                && (label.contains("EditionedFileId::parse")
                    || label.contains("parse_macro_expansion"))
        })
        .map(|(_, bytes)| *bytes)
        .sum()
}

#[test]
fn a_projection_keeps_every_tree_it_parses_unless_it_evicts() -> Result<()> {
    cordial::init_tracing();
    salsa::set_allocation_probe(counting_allocator::net_bytes);
    salsa::set_allocation_tags(salsa::AllocationTags {
        set_tag: counting_allocator::set_tag,
        held_by_tag: counting_allocator::held_by_tag,
        held_in_all: counting_allocator::held_in_all,
        peak_held: counting_allocator::peak_held,
        freed_pairs: counting_allocator::freed_pairs,
        freed_by_age: counting_allocator::freed_by_age,
    });
    let without = project(false)?;
    settle();
    let with = project(true)?;

    let (kept, bounded) = (
        trees_at_the_end_of_priming(&without),
        trees_at_the_end_of_priming(&with),
    );
    tracing::info!(
        target: "cordial::heap",
        "trees held at the end of priming: {} MB without evicting, {} MB with; {} evictions gave back {} MB once settled",
        kept / 1_000_000,
        bounded / 1_000_000,
        with.evictions,
        with.given_back / 1_000_000
    );
    // The run that is not asked to evict does not, and keeps the trees of the files it parsed.
    assert_eq!(without.evictions, 0, "a run not asked to evict evicted");
    assert!(
        kept > 150_000_000,
        "the run that does not evict holds {} MB of trees at the end of priming, where {FILES} files of about a megabyte were parsed",
        kept / 1_000_000
    );
    // The run that is asked to evicts between the chunks of the priming, and is given the memory.
    assert!(
        with.evictions >= 4,
        "{} evictions of {FILES} files",
        with.evictions
    );
    assert!(
        with.given_back > 100_000_000,
        "the evictions gave back {} MB once settled",
        with.given_back / 1_000_000
    );
    assert!(
        bounded * 2 < kept,
        "evicting between chunks left {} MB of trees at the end of priming, against {} MB without",
        bounded / 1_000_000,
        kept / 1_000_000
    );
    Ok(())
}

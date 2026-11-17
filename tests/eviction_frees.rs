//! What the queries of a run hold in memory, by query, and what evicting their values gives back.
//!
//! The program here counts the memory its threads hold and tags each allocation with the query
//! that was executing, so that what is held can be read by query at any point. The crate has more
//! files and more macro calls than the capacities rust-analyzer gives the syntax trees of files and
//! of expansions (128 and 512), so that evicting has values to drop.

#[path = "../src/counting_allocator.rs"]
mod counting_allocator;

use cordial::{CfgPlanning, CrateTarget, MetricsLayer, SavedState, SemanticCargoOptions};
use cordial::{SourceDatabase, StoreLayout};
use miette::{IntoDiagnostic, Result};
use std::path::{Path, PathBuf};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::SubscriberExt;

#[global_allocator]
static ALLOCATOR: counting_allocator::Counting = counting_allocator::Counting;

const FILES: usize = 140;
const STRUCTS_PER_FILE: usize = 3;

/// A crate of `FILES` files, each with types that derive two traits: more files than the capacity
/// of the parse of a file, and more macro calls than that of the parse of an expansion.
fn crate_with_derives() -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir(root.path().join("src")).into_diagnostic()?;
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname = \"evicted\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .into_diagnostic()?;
    let mut library = String::new();
    for file in 0..FILES {
        library.push_str(&format!("pub mod file{file};\n"));
        let mut text = String::new();
        for item in 0..STRUCTS_PER_FILE {
            text.push_str(&format!(
                "#[derive(Clone, Debug)]\npub struct S{file}_{item} {{ pub n: usize }}\n"
            ));
        }
        text.push_str(&format!(
            "pub fn f{file}() -> usize {{ S{file}_0 {{ n: 1 }}.clone().n }}\n"
        ));
        std::fs::write(root.path().join(format!("src/file{file}.rs")), text).into_diagnostic()?;
    }
    std::fs::write(root.path().join("src/lib.rs"), library).into_diagnostic()?;
    Ok(root)
}

/// The largest holders, a line each.
fn table(held: &[(String, i64)]) -> String {
    held.iter()
        .take(14)
        .map(|(name, bytes)| format!("  {:>8.2} MB  {name}\n", *bytes as f64 / 1e6))
        .collect()
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

/// The memory held is told by query, all of it is told, and evicting drops the values past the
/// capacities. (What dropping them frees is not asserted: on this crate it is nothing, since the
/// trees are held by something besides the values, and they are a small part of what is held.)
#[test]
fn what_the_queries_hold_is_told_by_query() -> Result<()> {
    cordial::init_tracing();
    salsa::set_allocation_probe(counting_allocator::net_bytes);
    salsa::set_allocation_tags(salsa::AllocationTags {
        set_tag: counting_allocator::set_tag,
        held_by_tag: counting_allocator::held_by_tag,
        held_in_all: counting_allocator::held_in_all,
        peak_held: counting_allocator::peak_held,
        freed_pairs: counting_allocator::freed_pairs,
    });
    cordial::start_heap_sampling();
    let root = crate_with_derives()?;
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = SavedState::at(state.path().join("analysis-state"));
    let subscriber = tracing_subscriber::registry()
        .with(MetricsLayer.with_filter(cordial::metrics_filter_with(true)));
    let _subscriber = tracing::subscriber::set_default(subscriber);
    let options = SemanticCargoOptions::default().with_cfg_planning(CfgPlanning::Explicit);
    let mut session =
        SourceDatabase::load_saved(root.path(), &options, Some(saved)).into_diagnostic()?;
    session
        .project(
            &CrateTarget::new("evicted", root.path()),
            &sources_of(root.path()),
        )
        .into_diagnostic()?;

    // The most the program held is told too, and by whom, with the stage it was in.
    let peak = |name: &str| {
        cordial::read_run_metrics(&{
            let store = tempfile::tempdir().expect("a directory");
            let layout = StoreLayout::from_root(store.path().to_path_buf(), "evicted".to_owned());
            cordial::write_run_metrics(&layout).expect("the metrics are written");
            std::mem::forget(store);
            layout
        })
        .values()
        .get(name)
        .copied()
    };
    let at_peak = peak("heap.peak_at.bytes").unwrap_or(0);
    assert!(
        i64::try_from(at_peak).unwrap_or(0) >= counting_allocator::held_in_all() * 9 / 10,
        "the highest the program held ({at_peak} bytes) is under what it holds now"
    );
    assert!(
        peak("heap.peak_held_crate_local_def_map.bytes").unwrap_or(0) > 0,
        "the highest is not told by query"
    );

    let held = session.held_by_ingredient();
    let told: i64 = held.iter().map(|(_, bytes)| bytes).sum();
    let all = counting_allocator::held_in_all();
    assert!(
        (told - all).abs() < 1_000_000,
        "the queries are told to hold {told} bytes of the {all} the program holds\n{}",
        table(&held)
    );
    let of = |name: &str| {
        held.iter()
            .find(|(held, _)| held.contains(name))
            .map_or(0, |(_, bytes)| *bytes)
    };
    for query in [
        "crate_local_def_map",
        "file_item_tree_query",
        "parse_macro_expansion",
    ] {
        assert!(
            of(query) > 0,
            "{query} is not told to hold anything\n{}",
            table(&held)
        );
    }
    assert!(
        of("outside queries") > 0,
        "what no query allocated is not told\n{}",
        table(&held)
    );

    session.evict_regenerable_now();
    let store = tempfile::tempdir().into_diagnostic()?;
    let layout = StoreLayout::from_root(store.path().to_path_buf(), "evicted".to_owned());
    cordial::write_run_metrics(&layout).into_diagnostic()?;
    let metrics = cordial::read_run_metrics(&layout);
    let dropped = |query: &str| {
        metrics
            .values()
            .get(&format!("eviction.{query}.dropped"))
            .copied()
            .unwrap_or(0)
    };
    for query in ["editionedfileid_parse", "macrocallid_parse_macro_expansion"] {
        assert!(dropped(query) > 0, "nothing of {query} was evicted");
    }
    Ok(())
}

/// What each query held less, from one reading to the next, the most first.
fn released(before: &[(String, i64)], after: &[(String, i64)]) -> Vec<(String, i64)> {
    let mut less: Vec<(String, i64)> = before
        .iter()
        .map(|(name, held)| {
            let now = after
                .iter()
                .find(|(other, _)| other == name)
                .map_or(0, |(_, bytes)| *bytes);
            (name.clone(), held - now)
        })
        .filter(|(_, less)| *less != 0)
        .collect();
    less.sort_by_key(|(_, less)| std::cmp::Reverse(*less));
    less
}

/// Evicting what is over the capacities gives back the memory of the syntax trees of expansions
/// and of files, on a crate with more of them than the capacities.
#[test]
fn evicting_gives_back_the_memory_of_the_trees() -> Result<()> {
    cordial::init_tracing();
    salsa::set_allocation_probe(counting_allocator::net_bytes);
    salsa::set_allocation_tags(salsa::AllocationTags {
        set_tag: counting_allocator::set_tag,
        held_by_tag: counting_allocator::held_by_tag,
        held_in_all: counting_allocator::held_in_all,
        peak_held: counting_allocator::peak_held,
        freed_pairs: counting_allocator::freed_pairs,
    });
    let root = crate_with_derives()?;
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = SavedState::at(state.path().join("analysis-state"));
    let options = SemanticCargoOptions::default().with_cfg_planning(CfgPlanning::Explicit);
    let mut session =
        SourceDatabase::load_saved(root.path(), &options, Some(saved)).into_diagnostic()?;
    session
        .project(
            &CrateTarget::new("evicted", root.path()),
            &sources_of(root.path()),
        )
        .into_diagnostic()?;
    let projected = session.held_by_ingredient();
    session.evict_regenerable_now();
    let less = released(&projected, &session.held_by_ingredient());
    let report = format!(
        "released:\n{}held before:\n{}",
        table(&less),
        table(&projected)
    );
    let of = |query: &str| {
        less.iter()
            .find(|(name, _)| name.contains(query))
            .map_or(0, |(_, bytes)| *bytes)
    };
    assert!(
        of("parse_macro_expansion") > 1_000_000,
        "the expansions' trees were not given back\n{report}"
    );
    assert!(
        of("EditionedFileId::parse") > 0,
        "the files' trees were not given back\n{report}"
    );
    Ok(())
}

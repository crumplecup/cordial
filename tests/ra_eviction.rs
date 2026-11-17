//! Whether evicting gives back the memory of the syntax trees rust-analyzer parsed, with
//! rust-analyzer and Salsa and nothing of cordial's.
//!
//! The second rung of the ladder that starts in `salsa_eviction.rs` (Salsa alone gives the memory
//! of evicted values back): the same measure, with the parse of a file as the query. The parse
//! keeps the 128 most recently used, so of a crate of more files than that the rest must be given
//! back when asked.

#[path = "../src/counting_allocator.rs"]
mod counting_allocator;

use ra_ap_hir::EditionedFileId;
use ra_ap_ide_db::base_db::relevant_crates;
use ra_ap_load_cargo::{LoadCargoConfig, ProcMacroServerChoice, load_workspace_at};
use ra_ap_project_model::CargoConfig;
use salsa::Database;
use tracing_subscriber::Layer;
use tracing_subscriber::layer::SubscriberExt;

#[global_allocator]
static ALLOCATOR: counting_allocator::Counting = counting_allocator::Counting;

/// More files than the parse keeps (128).
const FILES: usize = 220;
const KEPT: usize = 128;
const FUNCTIONS_PER_FILE: usize = 400;

fn held() -> i64 {
    counting_allocator::held_in_all()
}

/// A crate of `FILES` files of `FUNCTIONS_PER_FILE` functions each.
fn crate_of_large_files() -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("a directory");
    std::fs::create_dir(root.path().join("src")).expect("src");
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname = \"large\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("a manifest");
    let mut library = String::new();
    for file in 0..FILES {
        library.push_str(&format!("pub mod file{file};\n"));
        let mut text = String::new();
        for function in 0..FUNCTIONS_PER_FILE {
            text.push_str(&format!(
                "pub fn f{file}_{function}(a: usize, b: &str) -> usize {{\n    let c = a + b.len();\n    if c > {function} {{ c * 2 }} else {{ c + 1 }}\n}}\n"
            ));
        }
        std::fs::write(root.path().join(format!("src/file{file}.rs")), text).expect("a file");
    }
    std::fs::write(root.path().join("src/lib.rs"), library).expect("a library");
    root
}

#[test]
fn the_syntax_trees_past_the_capacity_are_given_back() {
    // The hooks are told to Salsa as in a run, which uses all of the allocator.
    salsa::set_allocation_probe(counting_allocator::net_bytes);
    salsa::set_allocation_tags(salsa::AllocationTags {
        set_tag: counting_allocator::set_tag,
        held_by_tag: counting_allocator::held_by_tag,
        held_in_all: counting_allocator::held_in_all,
        peak_held: counting_allocator::peak_held,
        freed_pairs: counting_allocator::freed_pairs,
        freed_by_age: counting_allocator::freed_by_age,
    });
    // What became of each value asked to be dropped is told by the events of Salsa's eviction.
    let subscriber = tracing_subscriber::registry()
        .with(cordial::MetricsLayer.with_filter(cordial::metrics_filter_with(false)));
    let _subscriber = tracing::subscriber::set_default(subscriber);
    let root = crate_of_large_files();
    let (mut db, vfs, _server) = load_workspace_at(
        root.path(),
        &CargoConfig::default(),
        &LoadCargoConfig {
            load_out_dirs_from_check: false,
            with_proc_macro_server: ProcMacroServerChoice::None,
            prefill_caches: false,
            num_worker_threads: 1,
            proc_macro_processes: 1,
        },
        &|_| {},
    )
    .expect("the workspace loads");
    let files: Vec<_> = vfs
        .iter()
        .filter(|(_, path)| path.to_string().contains("/src/file"))
        .map(|(file, _)| file)
        .collect();
    assert_eq!(files.len(), FILES);

    let before = held();
    for file in &files {
        let krate = *relevant_crates(&db, *file)
            .first()
            .expect("a crate has the file");
        let edition = krate.data(&db).edition;
        let parsed = EditionedFileId::new(&db, *file, edition).parse(&db);
        assert!(!parsed.syntax_node().text_range().is_empty());
    }
    let made = held() - before;
    db.trigger_lru_eviction();
    // Dropping a `Parse` hands its tree to a thread that frees it a little later (`ParseNodeDropper`
    // in rust-analyzer's syntax crate), so what is given back is what is held once that has run.
    let began = std::time::Instant::now();
    let mut back = made - (held() - before);
    let should =
        made * i64::try_from(FILES - KEPT).unwrap_or(0) / i64::try_from(FILES).unwrap_or(1);
    while back < should * 8 / 10 && began.elapsed() < std::time::Duration::from_secs(10) {
        std::thread::sleep(std::time::Duration::from_millis(20));
        back = made - (held() - before);
    }
    let settled_in = began.elapsed();
    let store = tempfile::tempdir().expect("a directory");
    let layout = cordial::StoreLayout::from_root(store.path().to_path_buf(), "ra".to_owned());
    cordial::write_run_metrics(&layout).expect("the metrics are written");
    let told: Vec<String> = cordial::read_run_metrics(&layout)
        .values()
        .iter()
        .filter(|(key, _)| key.starts_with("eviction.") && key.contains("parse"))
        .map(|(key, value)| format!("{key}={value}"))
        .collect();
    assert!(
        back >= should * 8 / 10,
        "parsing {FILES} files held {} MB; evicting gave back {} MB of the {} MB past the capacity after {settled_in:?}; {told:?}",
        made / 1_000_000,
        back / 1_000_000,
        should / 1_000_000
    );
}

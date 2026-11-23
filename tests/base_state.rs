//! A project with no state of its own starts from a base: a state saved once from a small workspace
//! that is only the toolchain's sysroot and a crate that exercises it. The project is loaded onto it
//! as it is onto its own last state, finds what a cold load finds, builds none of the sysroot's
//! definition maps again, and saves its own state sharing the base's chunks.

#[path = "support/identity_specimen.rs"]
mod identity_specimen;

use std::os::unix::fs::MetadataExt;
use std::path::Path;

use cordial::{
    MetricsLayer, SavedState, SemanticCargoOptions, SemanticItemId, SourceDatabase, StoreLayout,
    metrics_filter_with,
};
use identity_specimen::{caller_identities, project, specimen};
use miette::{IntoDiagnostic, Result};
use tracing_subscriber::{Layer, layer::SubscriberExt};

/// What the base is saved from: a crate that uses the parts of the standard library most crates use.
const BASE_LIB: &str = "\
use std::collections::HashMap;
pub fn warm(items: &[u32]) -> HashMap<u32, String> {
    items.iter().map(|item| (*item, item.to_string())).collect()
}
pub fn total(items: &[u32]) -> Option<u32> { items.iter().copied().max() }
";

fn base_project() -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir(root.path().join("src")).into_diagnostic()?;
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname = \"base_workload\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[workspace]\n",
    )
    .into_diagnostic()?;
    std::fs::write(root.path().join("src/lib.rs"), BASE_LIB).into_diagnostic()?;
    Ok(root)
}

/// What a graph says without the numbers of the identities in it, which are the database's own: the
/// same definitions in databases with different histories have different numbers. What comes before
/// its indexes as it is, and the words of the indexes sorted, as a hash map prints them in no order.
fn described(graph: &cordial::CrateIr) -> (String, Vec<String>) {
    let text = format!("{graph:?}");
    let (nodes, indexes) = text.split_once(" indexes: ").unwrap_or((&text, ""));
    let strip = |text: &str| {
        text.split(|c: char| c.is_ascii_digit())
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("#")
    };
    let mut words: Vec<String> = indexes
        .split(|c: char| !(c.is_alphabetic() || c == '_'))
        .filter(|word| !word.is_empty())
        .map(str::to_owned)
        .collect();
    words.sort();
    (strip(nodes), words)
}

/// One run: load (starting from `saved`), project the specimen or the base's crate, and say what the
/// graph holds.
fn run(root: &Path, saved: &SavedState, target: &str) -> Result<Run> {
    let mut session =
        SourceDatabase::load_saved(root, &SemanticCargoOptions::default(), Some(saved.clone()))
            .into_diagnostic()?;
    let restored = session.restored();
    let graph = if target == "base_workload" {
        session
            .project(
                &cordial::CrateTarget::new("base_workload", root),
                &[root.join("src/lib.rs")],
            )
            .into_diagnostic()?
    } else {
        project(&mut session, root)?
    };
    let identities = if target == "base_workload" {
        (Vec::new(), Vec::new())
    } else {
        caller_identities(&graph)?
    };
    Ok(Run {
        restored,
        graph: described(&graph),
        identities,
    })
}

/// What one run found: whether it started from a state, what the graph says, and the identities of
/// what `caller` calls and returns (numbers that mean something only to the lineage of states that
/// kept handing them on).
struct Run {
    restored: bool,
    graph: (String, Vec<String>),
    identities: (Vec<SemanticItemId>, Vec<SemanticItemId>),
}

/// How many definition maps Salsa has executed so far, from the metrics the run keeps.
fn definition_maps_executed(scratch: &Path) -> Result<u64> {
    let store = StoreLayout::from_root(scratch.join("store"), "specimen".to_owned());
    cordial::write_run_metrics(&store).into_diagnostic()?;
    let metrics = cordial::read_run_metrics(&store);
    Ok(metrics
        .values()
        .iter()
        .filter(|(name, _)| name.starts_with("steps.") && name.contains("crate_local_def_map"))
        .map(|(_, count)| *count)
        .sum())
}

fn chunk_inodes(state: &SavedState) -> Result<std::collections::BTreeMap<String, u64>> {
    let mut inodes = std::collections::BTreeMap::new();
    for entry in std::fs::read_dir(state.directory().join("salsa/chunks")).into_diagnostic()? {
        let entry = entry.into_diagnostic()?;
        inodes.insert(
            entry.file_name().to_string_lossy().into_owned(),
            entry.metadata().into_diagnostic()?.ino(),
        );
    }
    Ok(inodes)
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_project_started_from_a_base_finds_what_a_cold_load_finds_without_building_the_sysroot()
-> Result<()> {
    cordial::init_tracing();
    let scratch = tempfile::tempdir().into_diagnostic()?;
    let subscriber =
        tracing_subscriber::registry().with(MetricsLayer.with_filter(metrics_filter_with(true)));
    tracing::subscriber::with_default(subscriber, || -> Result<()> {
        // The base: saved once, from a workspace that is not the project.
        let base_root = base_project()?;
        let base = SavedState::base_at(scratch.path().join("base"), "base");
        let first = run(base_root.path(), &base, "base_workload")?;
        assert!(!first.restored, "nothing was saved before the base");
        let made = definition_maps_executed(scratch.path())?;
        assert!(made >= 20, "the base builds the sysroot's maps: {made}");
        let base_manifest =
            std::fs::read(base.directory().join("salsa/manifest")).into_diagnostic()?;

        // The project, loaded cold: what it finds, and the maps it builds.
        let root = specimen()?;
        let cold_state = SavedState::at(scratch.path().join("cold"));
        let cold = run(root.path(), &cold_state, "specimen")?;
        assert!(!cold.restored);
        let after_cold = definition_maps_executed(scratch.path())?;
        let cold_maps = after_cold - made;

        // The same project, with the base to start from.
        let state = SavedState::at(scratch.path().join("project")).with_base(base.directory());
        let from_base = run(root.path(), &state, "specimen")?;
        assert!(from_base.restored, "it started from the base");
        let from_base_maps = definition_maps_executed(scratch.path())? - after_cold;
        assert_eq!(
            from_base.graph, cold.graph,
            "the same graph, whatever the history of the database"
        );
        assert!(
            from_base_maps * 2 < cold_maps,
            "{from_base_maps} definition maps built from the base against {cold_maps} cold"
        );

        // What the project saved shares the base's chunks: linked, not written again.
        let base_chunks = chunk_inodes(&base)?;
        let project_chunks = chunk_inodes(&state)?;
        let shared = project_chunks
            .iter()
            .filter(|(name, inode)| base_chunks.get(*name) == Some(inode))
            .count();
        assert!(
            shared * 2 > project_chunks.len(),
            "{shared} of {} chunks of the project's state are the base's",
            project_chunks.len()
        );
        // And the base was only read.
        assert_eq!(
            std::fs::read(base.directory().join("salsa/manifest")).into_diagnostic()?,
            base_manifest
        );

        // The next run starts from the project's own state, not the base.
        let again = run(root.path(), &state, "specimen")?;
        assert!(again.restored);
        assert_eq!(again.graph, cold.graph);
        assert_eq!(
            again.identities, from_base.identities,
            "the identities the first run handed out are the ones the next finds"
        );
        Ok(())
    })
}

/// A directory whose path sorts before the toolchain's, as the paths of a project under `/dev/shm` or
/// of the registry's crates do: source roots are numbered in the order of their paths, so a root that
/// sorts before the sysroot's moves the sysroot's number. `None` where there is no such place.
fn early_directory() -> Option<tempfile::TempDir> {
    tempfile::tempdir_in("/dev/shm").ok()
}

/// The sysroot's source roots have the numbers they had in the base whatever the project's roots are:
/// a workspace of two packages has more roots than the base's one, all before the sysroot's, and what
/// was saved of the sysroot is still valid for it.
#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_project_with_more_roots_than_the_base_still_finds_the_sysroot_it_saved() -> Result<()> {
    cordial::init_tracing();
    let Some(early) = early_directory() else {
        // Without such a place the numbers cannot be made to differ here: nothing to check.
        return Ok(());
    };
    let scratch = tempfile::tempdir().into_diagnostic()?;
    let subscriber =
        tracing_subscriber::registry().with(MetricsLayer.with_filter(metrics_filter_with(true)));
    tracing::subscriber::with_default(subscriber, || -> Result<()> {
        // The base: one package, in a directory that sorts before the sysroot's.
        let base_root = early.path().join("base");
        std::fs::create_dir_all(base_root.join("src")).into_diagnostic()?;
        std::fs::write(
            base_root.join("Cargo.toml"),
            "[package]\nname = \"base_workload\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[workspace]\n",
        )
        .into_diagnostic()?;
        std::fs::write(base_root.join("src/lib.rs"), BASE_LIB).into_diagnostic()?;
        let base = SavedState::base_at(scratch.path().join("base"), "base");
        run(&base_root, &base, "base_workload")?;
        let made = definition_maps_executed(scratch.path())?;
        assert!(made >= 20, "the base builds the sysroot's maps: {made}");

        // The project: a workspace of two packages, so two roots before the sysroot's.
        let root = early.path().join("workspace");
        for member in ["first", "second"] {
            std::fs::create_dir_all(root.join(member).join("src")).into_diagnostic()?;
            std::fs::write(
                root.join(member).join("Cargo.toml"),
                format!(
                    "[package]\nname = \"{member}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n"
                ),
            )
            .into_diagnostic()?;
            std::fs::write(
                root.join(member).join("src/lib.rs"),
                "pub fn f() -> u32 { 1 }\n",
            )
            .into_diagnostic()?;
        }
        std::fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"first\", \"second\"]\nresolver = \"2\"\n",
        )
        .into_diagnostic()?;
        let state = SavedState::at(scratch.path().join("project")).with_base(base.directory());
        let mut session =
            SourceDatabase::load_saved(&root, &SemanticCargoOptions::default(), Some(state))
                .into_diagnostic()?;
        assert!(session.restored(), "it started from the base");
        session
            .project(
                &cordial::CrateTarget::new("first", root.join("first")),
                &[root.join("first/src/lib.rs")],
            )
            .into_diagnostic()?;
        let from_base = definition_maps_executed(scratch.path())? - made;
        assert!(
            from_base <= 4,
            "{from_base} definition maps executed from the base: the sysroot's are valid for a project with more roots"
        );
        Ok(())
    })
}

/// One ingredient of a saved Salsa state as its manifest lists it: index, name, source file, entries and
/// the chunks.
type Entry = (u32, String, String, u64, Vec<(String, u64)>);

fn manifest(state: &SavedState) -> Result<Vec<Entry>> {
    let bytes = std::fs::read(state.directory().join("salsa/manifest")).into_diagnostic()?;
    rmp_serde::from_slice(&bytes).into_diagnostic()
}

/// A base holds what the engine made and none of the tool's own memos, which are of the build that made
/// them; and a base that does not number the engine's ingredients as this build does is not used, and
/// the run starts cold and correct.
#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_base_holds_only_the_engines_ingredients_and_is_refused_by_a_build_that_numbers_them_otherwise()
-> Result<()> {
    cordial::init_tracing();
    let scratch = tempfile::tempdir().into_diagnostic()?;
    let base_root = base_project()?;
    let base = SavedState::base_at(scratch.path().join("base"), "base");
    run(base_root.path(), &base, "base_workload")?;

    let entries = manifest(&base)?;
    assert!(
        entries.len() > 20,
        "the engine's ingredients are in it: {}",
        entries.len()
    );
    let tools: Vec<_> = entries
        .iter()
        .filter(|(_, _, file, ..)| file.starts_with("src/"))
        .map(|(_, name, ..)| name.as_str())
        .collect();
    assert!(
        tools.is_empty(),
        "the tool's own ingredients are not: {tools:?}"
    );

    // As a build that numbered an ingredient differently would find it.
    let root = specimen()?;
    let honest = SavedState::at(scratch.path().join("honest")).with_base(base.directory());
    assert!(
        run(root.path(), &honest, "specimen")?.restored,
        "the base is used as saved"
    );

    let mut altered = entries;
    altered[0].1 = "an ingredient this build does not number so".to_owned();
    std::fs::write(
        base.directory().join("salsa/manifest"),
        rmp_serde::to_vec(&altered).into_diagnostic()?,
    )
    .into_diagnostic()?;
    let refused = SavedState::at(scratch.path().join("refused")).with_base(base.directory());
    let cold = run(root.path(), &refused, "specimen")?;
    assert!(
        !cold.restored,
        "a base that numbers an ingredient otherwise is not used"
    );
    let reference = run(
        root.path(),
        &SavedState::at(scratch.path().join("reference")),
        "specimen",
    )?;
    assert_eq!(cold.graph, reference.graph, "and the run is the cold run");
    Ok(())
}

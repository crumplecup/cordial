//! A cfg context added as a copy of the workspace coexists with the ordinary build
//! in one database: its functions are the ones its cfg activates, the ordinary
//! crates and files keep what they had, and nothing Salsa memoised for them is
//! computed again.

mod support {
    pub mod semantic_metrics;
}
use cordial::{
    CfgPlanning, CrateTarget, FunctionLocation, SemanticCargoOptions, SemanticContextKey,
    SourceDatabase,
};
use miette::{IntoDiagnostic, Result};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Locations as (file, offset), which a test can write down.
type Spots = BTreeSet<(PathBuf, u32)>;

fn spots(locations: BTreeSet<FunctionLocation>) -> Spots {
    locations
        .into_iter()
        .map(|location| (location.file().clone(), *location.offset()))
        .collect()
}
use tracing_subscriber::layer::SubscriberExt;

const LIBRARY: &str = "\
pub fn helper() -> Result<(), std::io::Error> { Ok(()) }

#[cfg(kani)]
pub fn harness() { let _ = helper(); }

#[cfg(not(kani))]
pub fn ordinary_only() {}
";

fn kani() -> SemanticContextKey {
    SemanticContextKey::enabling(["kani"])
}

fn fixture() -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir_all(root.path().join("src")).into_diagnostic()?;
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='gated'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    std::fs::write(root.path().join("src/lib.rs"), LIBRARY).into_diagnostic()?;
    Ok(root)
}

/// The first place two texts differ, with a little context from each.
fn first_difference(left: &str, right: &str) -> String {
    let at = left
        .bytes()
        .zip(right.bytes())
        .position(|(a, b)| a != b)
        .unwrap_or(left.len().min(right.len()));
    let around = |text: &str| {
        let from = at.saturating_sub(200);
        text.get(from..(at + 200).min(text.len()))
            .unwrap_or_default()
            .to_owned()
    };
    format!(
        "at byte {at}:\n  before: {}\n  after:  {}",
        around(left),
        around(right)
    )
}

/// Where the function whose source starts with `marker` begins in `lib.rs`.
fn at(root: &Path, marker: &str) -> Result<(PathBuf, u32)> {
    let start = LIBRARY
        .find(marker)
        .ok_or_else(|| miette::miette!("{marker} is not in the fixture"))?;
    Ok((
        root.join("src/lib.rs"),
        u32::try_from(start).into_diagnostic()?,
    ))
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_context_is_crates_beside_the_ordinary_ones_and_leaves_them_alone() -> Result<()> {
    cordial::init_tracing();
    let metrics = support::semantic_metrics::QueryMetrics::default();
    let subscriber = tracing_subscriber::registry()
        .with(metrics.clone())
        .with(cordial::tracing_fmt_layer());
    let _subscriber = tracing::subscriber::set_default(subscriber);
    let root = fixture()?;
    let options = SemanticCargoOptions::default().with_cfg_planning(CfgPlanning::Explicit);
    let mut session = SourceDatabase::load_with(root.path(), &options).into_diagnostic()?;
    let target = CrateTarget::new("gated", root.path());
    let paths = [root.path().join("src/lib.rs")];

    let first = session.project(&target, &paths).into_diagnostic()?;
    let before = metrics.snapshot();
    let ordinary_before = spots(session.ordinary_functions());
    assert!(ordinary_before.contains(&at(root.path(), "#[cfg(not(kani))]\npub fn ordinary_only")?));
    assert!(
        !ordinary_before.contains(&at(root.path(), "#[cfg(kani)]\npub fn harness")?),
        "the ordinary build does not see code gated on kani"
    );

    // The context's crates see exactly what its cfg activates.
    let added = session.add_context_crates(&kani()).into_diagnostic()?;
    let in_kani = spots(session.context_functions(&added));
    assert!(in_kani.contains(&at(root.path(), "#[cfg(kani)]\npub fn harness")?));
    assert!(in_kani.contains(&at(root.path(), "pub fn helper")?));
    assert!(
        !in_kani.contains(&at(root.path(), "#[cfg(not(kani))]\npub fn ordinary_only")?),
        "code gated off by the context is inactive in it"
    );

    // Adding it again is the same context, and the ordinary crates are as they were.
    assert_eq!(
        session
            .add_context_crates(&kani())
            .into_diagnostic()?
            .context(),
        added.context()
    );
    assert_eq!(spots(session.ordinary_functions()), ordinary_before);

    // Projecting the ordinary build again gives what it gave.
    let again = session.project(&target, &paths).into_diagnostic()?;
    // Compared as values: the snapshot's indexes are hash maps, which serialise
    // their keys in a different order from one instance to the next.
    let (left, right) = (
        serde_json::to_value(first.snapshot().into_diagnostic()?).into_diagnostic()?,
        serde_json::to_value(again.snapshot().into_diagnostic()?).into_diagnostic()?,
    );
    assert!(
        left == right,
        "the ordinary projection is unchanged by a context's crates: {}",
        first_difference(&left.to_string(), &right.to_string())
    );

    // The context has files and crates of its own, so adding it cost the ordinary
    // build nothing: not one query, of any file or function, ran again.
    let after = metrics.snapshot();
    assert_eq!(
        after, before,
        "adding a context's crates re-ran queries for the ordinary build"
    );

    // A later projection, with the context already added, computes nothing.
    session.project(&target, &paths).into_diagnostic()?;
    assert_eq!(
        metrics.snapshot(),
        after,
        "a context already added costs nothing on the next projection"
    );
    Ok(())
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn an_edit_reaches_the_contexts_copy_of_the_file() -> Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let options = SemanticCargoOptions::default().with_cfg_planning(CfgPlanning::Explicit);
    let mut session = SourceDatabase::load_with(root.path(), &options).into_diagnostic()?;
    let added = session.add_context_crates(&kani()).into_diagnostic()?;
    let lib = root.path().join("src/lib.rs");
    let marker = "#[cfg(kani)]\npub fn second_harness() {}";
    let edited = format!("{LIBRARY}\n{marker}\n");
    let at = (
        lib.clone(),
        u32::try_from(
            edited
                .find(marker)
                .ok_or_else(|| miette::miette!("no marker"))?,
        )
        .into_diagnostic()?,
    );
    assert!(!spots(session.context_functions(&added)).contains(&at));

    session.change_file(&lib, edited).into_diagnostic()?;

    assert!(
        spots(session.context_functions(&added)).contains(&at),
        "the context's copy of lib.rs did not follow the edit"
    );
    assert!(
        !spots(session.ordinary_functions()).contains(&at),
        "the ordinary build must not see code gated on kani"
    );
    Ok(())
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn an_edit_synced_from_disk_reaches_the_contexts_copy_too() -> Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let options = SemanticCargoOptions::default().with_cfg_planning(CfgPlanning::Explicit);
    let mut session = SourceDatabase::load_with(root.path(), &options).into_diagnostic()?;
    let added = session.add_context_crates(&kani()).into_diagnostic()?;
    let lib = root.path().join("src/lib.rs");
    let marker = "#[cfg(kani)]\npub fn synced_harness() {}";
    let edited = format!("{LIBRARY}\n{marker}\n");
    let at = (
        lib.clone(),
        u32::try_from(
            edited
                .find(marker)
                .ok_or_else(|| miette::miette!("no marker"))?,
        )
        .into_diagnostic()?,
    );
    std::fs::write(&lib, &edited).into_diagnostic()?;

    let outcome = session.sync_from_disk().into_diagnostic()?;
    assert!(
        matches!(outcome, cordial::SyncOutcome::Updated(_)),
        "{outcome:?}"
    );
    assert!(
        spots(session.context_functions(&added)).contains(&at),
        "the context's copy did not follow the file the session synced from disk"
    );
    Ok(())
}

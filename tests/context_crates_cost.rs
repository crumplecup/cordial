//! A context copies the crates it changes and the dependencies on the way to them, and no more. What it
//! costs for the crates it does not change is nothing: they are the ordinary crates, whose maps are
//! built and shared. And it finds what copying every crate finds.

use cordial::{
    CrateTarget, MetricsLayer, SemanticCargoOptions, SourceDatabase, StoreLayout,
    metrics_filter_with,
};
use miette::{IntoDiagnostic, Result};
use tracing_subscriber::{Layer, layer::SubscriberExt};

const SMALL: usize = 3;
const LARGE: usize = 9;

fn write_crate(
    root: &std::path::Path,
    name: &str,
    dependencies: &[&str],
    source: &str,
) -> Result<()> {
    let dir = root.join(name);
    std::fs::create_dir_all(dir.join("src")).into_diagnostic()?;
    let deps: String = dependencies
        .iter()
        .map(|dependency| format!("{dependency} = {{ path = \"../{dependency}\" }}\n"))
        .collect();
    std::fs::write(
        dir.join("Cargo.toml"),
        format!(
            "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[features]\nextra = []\n[dependencies]\n{deps}"
        ),
    )
    .into_diagnostic()?;
    std::fs::write(dir.join("src/lib.rs"), source).into_diagnostic()?;
    Ok(())
}

fn write_workspace(root: &std::path::Path, members: &[String]) -> Result<()> {
    let quoted: Vec<String> = members
        .iter()
        .map(|member| format!("\"{member}\""))
        .collect();
    std::fs::write(
        root.join("Cargo.toml"),
        format!(
            "[workspace]\nmembers = [{}]\nresolver = \"2\"\n",
            quoted.join(", ")
        ),
    )
    .into_diagnostic()
}

/// A workspace of `crates` independent library crates, the first with a gate a context flips.
fn independent(crates: usize) -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    let members: Vec<String> = (0..crates).map(|index| format!("c{index}")).collect();
    write_workspace(root.path(), &members)?;
    for index in 0..crates {
        let gated = if index == 0 {
            "#[cfg(feature = \"extra\")]\npub fn only_with_extra() -> u32 { 7 }\n"
        } else {
            ""
        };
        write_crate(
            root.path(),
            &format!("c{index}"),
            &[],
            // A `cfg_attr` the context does not flip, as most are (a derive under a feature it does not
            // turn on): it must not make the crate changed.
            &format!(
                "#[cfg_attr(feature = \"serde\", derive(Clone))]\npub struct S{index};\npub fn f{index}() -> u32 {{ {index} }}\npub fn g{index}() -> u32 {{ f{index}() + 1 }}\n{gated}"
            ),
        )?;
    }
    Ok(root)
}

/// `top` (gated) depends on `mid` (not) which depends on `base` (gated); `other` depends on `base` and
/// is not changed by anything.
fn chain() -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    let members: Vec<String> = ["base", "mid", "top", "other"].map(String::from).into();
    write_workspace(root.path(), &members)?;
    write_crate(
        root.path(),
        "base",
        &[],
        "pub fn plain() -> u32 { 1 }\n#[cfg(feature = \"extra\")]\npub fn only_with_extra() -> u32 { 2 }\n",
    )?;
    write_crate(
        root.path(),
        "mid",
        &["base"],
        "pub fn through() -> u32 { base::plain() }\n",
    )?;
    write_crate(
        root.path(),
        "top",
        &["mid", "base"],
        "pub fn top() -> u32 { mid::through() }\n#[cfg(feature = \"extra\")]\npub fn top_extra() -> u32 { base::only_with_extra() }\n",
    )?;
    write_crate(
        root.path(),
        "other",
        &["base"],
        "pub fn o() -> u32 { base::plain() }\n",
    )?;
    Ok(root)
}

fn metrics_of(scratch: &std::path::Path) -> Result<cordial::RunMetrics> {
    let store = StoreLayout::from_root(scratch.join("store"), "specimen".to_owned());
    cordial::write_run_metrics(&store).into_diagnostic()?;
    Ok(cordial::read_run_metrics(&store))
}

/// How many definition maps were executed under the contexts so far.
fn context_maps(metrics: &cordial::RunMetrics) -> u64 {
    metrics
        .values()
        .iter()
        .filter(|(name, _)| {
            name.starts_with("steps.")
                && name.contains("crate_local_def_map")
                && name.contains("analyse_contexts")
        })
        .map(|(_, count)| *count)
        .sum()
}

fn project(root: &std::path::Path, target: &str, every: bool) -> Result<String> {
    let options = SemanticCargoOptions::default().with_copy_every_crate(every);
    let mut session = SourceDatabase::load_with(root, &options).into_diagnostic()?;
    let dir = root.join(target);
    let graph = session
        .project(&CrateTarget::new(target, &dir), &[dir.join("src/lib.rs")])
        .into_diagnostic()?;
    Ok(format!("{graph:?}"))
}

/// What a graph says without the numbers of its identities.
fn described(text: &str) -> (String, Vec<String>) {
    let (nodes, indexes) = text.split_once(" indexes: ").unwrap_or((text, ""));
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

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_context_builds_no_definition_map_for_a_crate_it_does_not_change() -> Result<()> {
    cordial::init_tracing();
    let scratch = tempfile::tempdir().into_diagnostic()?;
    let subscriber =
        tracing_subscriber::registry().with(MetricsLayer.with_filter(metrics_filter_with(true)));
    let (small, large, every) = tracing::subscriber::with_default(subscriber, || -> Result<_> {
        project(independent(SMALL)?.path(), "c0", false)?;
        let small = context_maps(&metrics_of(scratch.path())?);
        project(independent(LARGE)?.path(), "c0", false)?;
        let large = context_maps(&metrics_of(scratch.path())?) - small;
        project(independent(LARGE)?.path(), "c0", true)?;
        let every = context_maps(&metrics_of(scratch.path())?) - small - large;
        Ok((small, large, every))
    })?;
    // Every crate copied, as it was: a definition map for each. Copying what it changes: the same
    // number however many crates it does not change.
    assert!(
        every >= LARGE as u64,
        "copying every crate builds a map for each: {every}"
    );
    assert_eq!(
        large, small,
        "{small} definition maps under the context with {SMALL} crates and {large} with {LARGE}"
    );
    assert!(
        large < every,
        "{large} against {every} when every crate is copied"
    );
    Ok(())
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_context_that_copies_only_what_it_changes_finds_what_copying_every_crate_finds() -> Result<()> {
    cordial::init_tracing();
    let scratch = tempfile::tempdir().into_diagnostic()?;
    let subscriber =
        tracing_subscriber::registry().with(MetricsLayer.with_filter(metrics_filter_with(true)));
    tracing::subscriber::with_default(subscriber, || -> Result<()> {
        let root = chain()?;
        let every = project(root.path(), "top", true)?;
        let copied_every = metrics_of(scratch.path())?
            .values()
            .get("semantic.contexts.crates_copied")
            .copied()
            .unwrap_or(0);
        let only = project(root.path(), "top", false)?;
        let copied_only = metrics_of(scratch.path())?
            .values()
            .get("semantic.contexts.crates_copied")
            .copied()
            .unwrap_or(0)
            - copied_every;
        assert!(
            every.contains("top_extra"),
            "the function only the context has is found, as the gate flips: {every}"
        );
        assert_eq!(described(&only), described(&every), "the same graph");
        // `top` and `base` are changed, `mid` is on the way from one to the other, `other` depends on a
        // changed crate and is changed by nothing.
        assert_eq!(copied_every, 4, "every crate");
        assert_eq!(
            copied_only, 3,
            "all but the crate that only depends on a changed one"
        );
        Ok(())
    })
}

/// A `cfg_attr` whose predicate the context flips changes the file's attributes there, so it makes its
/// crate changed; one it does not flip does not.
#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_cfg_attr_the_context_flips_makes_its_crate_changed_and_one_it_does_not_flip_does_not()
-> Result<()> {
    cordial::init_tracing();
    let scratch = tempfile::tempdir().into_diagnostic()?;
    let subscriber =
        tracing_subscriber::registry().with(MetricsLayer.with_filter(metrics_filter_with(true)));
    tracing::subscriber::with_default(subscriber, || -> Result<()> {
        let root = independent(4)?;
        // The second crate's `cfg_attr` is on the feature the context turns on.
        let lib = root.path().join("c1/src/lib.rs");
        let text = std::fs::read_to_string(&lib).into_diagnostic()?;
        std::fs::write(
            &lib,
            text.replace("feature = \"serde\"", "feature = \"extra\""),
        )
        .into_diagnostic()?;
        project(root.path(), "c0", false)?;
        let copied = metrics_of(scratch.path())?
            .values()
            .get("semantic.contexts.crates_copied")
            .copied()
            .unwrap_or(0);
        assert_eq!(
            copied, 2,
            "the crate with the gate and the one whose cfg_attr flips: not the other two"
        );
        Ok(())
    })
}

/// `a` defines the same name under `kani` and under `creusot`, so the two cannot be one context; `b` is
/// gated on `creusot` and `both` on the two together. The contexts are `kani`, `creusot` and both, and
/// `b`'s copy under `creusot` is the copy it has under both (and under the two together that is first
/// tried and dissolved), since it says nothing of `kani`.
fn nested() -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    let members: Vec<String> = ["a", "b", "both"].map(String::from).into();
    write_workspace(root.path(), &members)?;
    write_crate(
        root.path(),
        "a",
        &[],
        "pub fn plain_a() -> u32 { 1 }\n#[cfg(kani)]\npub fn alternative() -> u32 { 2 }\n#[cfg(creusot)]\npub fn alternative() -> u32 { 3 }\n",
    )?;
    write_crate(
        root.path(),
        "b",
        &[],
        "pub fn plain_b() -> u32 { 3 }\n#[cfg(creusot)]\npub fn only_creusot() -> u32 { 4 }\n",
    )?;
    write_crate(
        root.path(),
        "both",
        &["a", "b"],
        "pub fn plain() -> u32 { a::plain_a() + b::plain_b() }\n#[cfg(all(kani, creusot))]\npub fn only_both() -> u32 { 5 }\n",
    )?;
    Ok(root)
}

fn project_sharing(root: &std::path::Path, target: &str, share: bool) -> Result<String> {
    let options = SemanticCargoOptions::default().with_do_not_share_context_copies(!share);
    let mut session = SourceDatabase::load_with(root, &options).into_diagnostic()?;
    let dir = root.join(target);
    let graph = session
        .project(&CrateTarget::new(target, &dir), &[dir.join("src/lib.rs")])
        .into_diagnostic()?;
    Ok(format!("{graph:?}"))
}

/// Contexts that differ in what a crate never names share that crate's copy: its definition map is built
/// once, not once for each context, and what the contexts find is the same.
#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn contexts_that_differ_in_what_a_crate_never_names_share_its_copy() -> Result<()> {
    cordial::init_tracing();
    let scratch = tempfile::tempdir().into_diagnostic()?;
    let subscriber =
        tracing_subscriber::registry().with(MetricsLayer.with_filter(metrics_filter_with(true)));
    let (alone, shared, alone_graph, shared_graph) =
        tracing::subscriber::with_default(subscriber, || -> Result<_> {
            let root = nested()?;
            let alone_graph = project_sharing(root.path(), "both", false)?;
            let alone = context_maps(&metrics_of(scratch.path())?);
            let shared_graph = project_sharing(root.path(), "both", true)?;
            let shared = context_maps(&metrics_of(scratch.path())?) - alone;
            Ok((alone, shared, alone_graph, shared_graph))
        })?;
    assert!(
        alone_graph.contains("only_both"),
        "the function only the third context has is found: {alone_graph}"
    );
    assert_eq!(
        described(&shared_graph),
        described(&alone_graph),
        "the same graph"
    );
    assert!(
        shared < alone,
        "{shared} definition maps under the contexts when copies are shared, {alone} when not"
    );
    Ok(())
}

/// A test file is read by the leaf pass, in the contexts that have `test` on; a cfg context that does not
/// flip anything in its crate copies neither it nor the crate.
#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_cfg_context_does_not_copy_a_crate_for_its_test_files() -> Result<()> {
    cordial::init_tracing();
    let scratch = tempfile::tempdir().into_diagnostic()?;
    let subscriber =
        tracing_subscriber::registry().with(MetricsLayer.with_filter(metrics_filter_with(true)));
    tracing::subscriber::with_default(subscriber, || -> Result<()> {
        let root = independent(3)?;
        let tests = root.path().join("c1/tests");
        std::fs::create_dir_all(&tests).into_diagnostic()?;
        std::fs::write(
            tests.join("t.rs"),
            "#[test]\nfn t() { assert_eq!(c1::f1(), 1); }\n",
        )
        .into_diagnostic()?;
        project(root.path(), "c0", false)?;
        let copied = metrics_of(scratch.path())?
            .values()
            .get("semantic.contexts.crates_copied")
            .copied()
            .unwrap_or(0);
        assert_eq!(copied, 1, "only the crate whose gate the context flips");
        Ok(())
    })
}

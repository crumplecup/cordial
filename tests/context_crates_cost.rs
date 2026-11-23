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
        // `top` and `base` are changed. `mid` is on the way from one to the other, but mentions nothing the
        // flip in `base` makes visible, so `top` sees it as the ordinary build has it: it is not copied. `other`
        // depends on a changed crate and is changed by nothing.
        assert_eq!(copied_every, 4, "every crate");
        assert_eq!(
            copied_only, 2,
            "the crates the gates change, and not what only depends on them"
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
    let members: Vec<String> = ["a", "b", "d", "both"].map(String::from).into();
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
        "d",
        &["a"],
        "pub fn plain_d() -> u32 { a::plain_a() }\n#[cfg(creusot)]\npub fn d_only() -> u32 { 6 }\n",
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
    // `b` (which names only `creusot`) and `d` (the same, and a dependent of `a`, whose copies differ
    // between the contexts but which mentions nothing they switch) are each built once and not for every
    // context that has `creusot` on.
    assert!(
        shared + 2 <= alone,
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

/// A file with no module in the ordinary build, because the `mod` that declares it is gated off, changes
/// in a context only where that gate flips: a crate with one gated on something the context leaves alone
/// is not copied, and one gated on what the context turns on is.
#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_file_with_no_module_changes_only_where_the_gate_that_hides_it_flips() -> Result<()> {
    cordial::init_tracing();
    let scratch = tempfile::tempdir().into_diagnostic()?;
    let subscriber =
        tracing_subscriber::registry().with(MetricsLayer.with_filter(metrics_filter_with(true)));
    tracing::subscriber::with_default(subscriber, || -> Result<()> {
        let root = independent(4)?;
        for (krate, feature) in [("c1", "never"), ("c2", "extra")] {
            let lib = root.path().join(krate).join("src/lib.rs");
            let text = std::fs::read_to_string(&lib).into_diagnostic()?;
            std::fs::write(
                &lib,
                format!("#[cfg(feature = \"{feature}\")]\nmod hidden;\n{text}"),
            )
            .into_diagnostic()?;
            std::fs::write(lib.with_file_name("hidden.rs"), "pub fn h() -> u32 { 1 }\n")
                .into_diagnostic()?;
        }
        project(root.path(), "c0", false)?;
        let copied = metrics_of(scratch.path())?
            .values()
            .get("semantic.contexts.crates_copied")
            .copied()
            .unwrap_or(0);
        // The gates ask for two contexts, `extra` and `never`. `extra` copies the crate with the gate and the
        // one whose hidden file it reveals; `never` copies the one it gates. Before, each also copied every
        // crate with a hidden file: five.
        assert_eq!(copied, 3, "the crates a context's own gates change");
        Ok(())
    })
}

/// The queries of one name executed under the contexts so far, by the step that asked.
fn context_queries(metrics: &cordial::RunMetrics, query: &str) -> u64 {
    metrics
        .values()
        .iter()
        .filter(|(name, _)| {
            name.starts_with("steps.")
                && name.ends_with(&format!("> {query}"))
                && name.contains("analyse_contexts")
        })
        .map(|(_, count)| *count)
        .sum()
}

/// A crate with a gate a context flips, and a file the context does not change that calls a macro `CALLS`
/// times: the file is taken from the ordinary build as it is, so what the macro expands to is the ordinary
/// build's too.
fn macros_in_an_unchanged_file(calls: usize) -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    write_workspace(root.path(), &["m".to_owned()])?;
    write_crate(
        root.path(),
        "m",
        &[],
        "mod calls;\n#[cfg(feature = \"extra\")]\npub fn only_with_extra() -> u32 { 1 }\n",
    )?;
    let body: String = (0..calls)
        .map(|index| format!("pub fn f{index}() -> u32 {{ twice!({index}) }}\n"))
        .collect();
    std::fs::write(
        root.path().join("m/src/calls.rs"),
        format!("macro_rules! twice {{ ($e:expr) => {{ $e + $e }}; }}\n{body}"),
    )
    .into_diagnostic()?;
    Ok(root)
}

/// What a context costs for the macro calls of a file it takes as it is: the expansions of the ordinary
/// build are the same, and are not made again. This says how many are, at two sizes.
#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_context_expands_no_macro_in_a_file_it_takes_as_it_is() -> Result<()> {
    cordial::init_tracing();
    let scratch = tempfile::tempdir().into_diagnostic()?;
    let subscriber =
        tracing_subscriber::registry().with(MetricsLayer.with_filter(metrics_filter_with(true)));
    let (few, many) = tracing::subscriber::with_default(subscriber, || -> Result<_> {
        project(macros_in_an_unchanged_file(2)?.path(), "m", false)?;
        let few = context_queries(&metrics_of(scratch.path())?, "expand_pure_macro_of");
        project(macros_in_an_unchanged_file(12)?.path(), "m", false)?;
        let many = context_queries(&metrics_of(scratch.path())?, "expand_pure_macro_of") - few;
        Ok((few, many))
    })?;
    // The count is of a query that runs: the ordinary build expands them.
    let everywhere: u64 = metrics_of(scratch.path())?
        .values()
        .iter()
        .filter(|(name, _)| name.starts_with("steps.") && name.ends_with("> expand_pure_macro_of"))
        .map(|(_, count)| *count)
        .sum();
    assert!(
        everywhere >= 14,
        "the ordinary build expands the calls: {everywhere}"
    );
    assert_eq!(
        (few, many),
        (0, 0),
        "macro expansions executed under the context, with 2 calls and with 12"
    );
    Ok(())
}

/// A package's integration tests are crates that share one directory and each file is the root of one: a
/// gate a context flips in one of them changes that crate and not its siblings.
#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_gate_in_one_integration_test_changes_that_test_and_not_the_others() -> Result<()> {
    cordial::init_tracing();
    let scratch = tempfile::tempdir().into_diagnostic()?;
    let subscriber =
        tracing_subscriber::registry().with(MetricsLayer.with_filter(metrics_filter_with(true)));
    tracing::subscriber::with_default(subscriber, || -> Result<()> {
        let root = independent(1)?;
        let tests = root.path().join("c0/tests");
        std::fs::create_dir_all(&tests).into_diagnostic()?;
        std::fs::write(
            tests.join("a.rs"),
            "#[cfg(feature = \"extra\")]\n#[test]\nfn gated() {}\n#[test]\nfn plain_a() {}\n",
        )
        .into_diagnostic()?;
        for name in ["b", "c", "d"] {
            std::fs::write(tests.join(format!("{name}.rs")), "#[test]\nfn plain() {}\n")
                .into_diagnostic()?;
        }
        project(root.path(), "c0", false)?;
        let copied = metrics_of(scratch.path())?
            .values()
            .get("semantic.contexts.crates_copied")
            .copied()
            .unwrap_or(0);
        // Three contexts: `extra` copies the library, whose gate it flips, and the test that has one of its
        // own, not the three that have none (2); the first leaf context reads every test (4); the leaf
        // context with `extra` on copies the library and the test with a gate, and not the three tests that
        // name nothing a gate switched (2). A directory-wide ownership copied every test in each (11).
        assert_eq!(copied, 8, "crates copied over the contexts");
        Ok(())
    })
}

/// A crate with a gate a context flips, and a file the context does not change that has `ITEMS` derives and
/// item macro calls: the definition map of the context's copy of the crate expands them, as the ordinary
/// crate's did.
fn item_macros_in_an_unchanged_file(items: usize) -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    write_workspace(root.path(), &["m".to_owned()])?;
    write_crate(
        root.path(),
        "m",
        &[],
        "mod items;\n#[cfg(feature = \"extra\")]\npub fn only_with_extra() -> u32 { 1 }\n",
    )?;
    let body: String = (0..items)
        .map(|index| {
            format!("#[derive(Clone, Debug, PartialEq)]\npub struct S{index} {{ pub a: u32 }}\nmake!(f{index});\n")
        })
        .collect();
    std::fs::write(
        root.path().join("m/src/items.rs"),
        format!(
            "macro_rules! make {{ ($name:ident) => {{ pub fn $name() -> u32 {{ 1 }} }}; }}\n{body}"
        ),
    )
    .into_diagnostic()?;
    Ok(root)
}

/// How many expansions of item-position macro calls and derives a context makes in a file it takes as it is,
/// at two sizes: the ordinary build's are the same expansions.
#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_context_expands_the_item_macros_of_a_file_it_takes_as_it_is() -> Result<()> {
    cordial::init_tracing();
    let scratch = tempfile::tempdir().into_diagnostic()?;
    let subscriber =
        tracing_subscriber::registry().with(MetricsLayer.with_filter(metrics_filter_with(true)));
    let (few, many, ordinary) = tracing::subscriber::with_default(subscriber, || -> Result<_> {
        project(item_macros_in_an_unchanged_file(2)?.path(), "m", false)?;
        let metrics = metrics_of(scratch.path())?;
        let few = context_queries(&metrics, "expand_pure_macro_of");
        let ordinary: u64 = metrics
            .values()
            .iter()
            .filter(|(name, _)| {
                name.starts_with("steps.") && name.ends_with("> expand_pure_macro_of")
            })
            .map(|(_, count)| *count)
            .sum();
        project(item_macros_in_an_unchanged_file(12)?.path(), "m", false)?;
        let many = context_queries(&metrics_of(scratch.path())?, "expand_pure_macro_of") - few;
        Ok((few, many, ordinary))
    })?;
    assert_eq!(
        (few, many),
        (0, 0),
        "expansions of item macros executed under the context at 2 and at 12 items ({ordinary} in all at 2)"
    );
    Ok(())
}

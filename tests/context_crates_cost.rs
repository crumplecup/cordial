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

/// A crate with a gate a context flips in its root, and `modules` modules in files of their own, each with a
/// struct, an `impl`, a `use` of another module's struct and a function.
fn modules_beside_a_gate(modules: usize) -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    write_workspace(root.path(), &["m".to_owned()])?;
    let declared: String = (0..modules)
        .map(|index| format!("mod m{index};\n"))
        .collect();
    write_crate(
        root.path(),
        "m",
        &[],
        &format!(
            "{declared}#[cfg(feature = \"extra\")]\npub fn only_with_extra() -> u32 {{ 1 }}\n"
        ),
    )?;
    for index in 0..modules {
        let other = (index + 1) % modules;
        std::fs::write(
            root.path().join(format!("m/src/m{index}.rs")),
            format!(
                "use crate::m{other}::S{other};\npub struct S{index};\nimpl S{index} {{ pub fn f(&self) -> u32 {{ {index} }} }}\npub fn g{index}(_: S{other}) -> u32 {{ {index} }}\n"
            ),
        )
        .into_diagnostic()?;
    }
    Ok(root)
}

/// What a context does for the item trees of a crate it changes in one gate, when the crate has more modules the
/// gate does not touch: the files it takes as they are are lowered once, for the ordinary crate, and not again for
/// the copy.
#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_contexts_item_trees_do_not_grow_with_the_modules_its_gate_does_not_touch() -> Result<()> {
    cordial::init_tracing();
    let scratch = tempfile::tempdir().into_diagnostic()?;
    let subscriber =
        tracing_subscriber::registry().with(MetricsLayer.with_filter(metrics_filter_with(true)));
    let (few, many) = tracing::subscriber::with_default(subscriber, || -> Result<_> {
        project(modules_beside_a_gate(3)?.path(), "m", false)?;
        let few = context_queries(&metrics_of(scratch.path())?, "file_item_tree_query");
        project(modules_beside_a_gate(30)?.path(), "m", false)?;
        let many = context_queries(&metrics_of(scratch.path())?, "file_item_tree_query") - few;
        Ok((few, many))
    })?;
    assert!(
        many <= few + 3,
        "{few} item trees lowered under the context with 3 modules and {many} with 30"
    );
    Ok(())
}

/// The sum of a numeric field of the events that say which modules a copy takes from the original.
fn sharing_field(metrics: &cordial::RunMetrics, field: &str) -> u64 {
    metrics
        .values()
        .iter()
        .filter(|(name, _)| name.ends_with(&format!("def_map.{field}")))
        .map(|(_, count)| *count)
        .sum()
}

/// Which of a copy's modules are the original's: with the gate in the root the only module the context affects, the
/// modules beside it are all taken from the original, however many there are.
#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_copy_takes_the_modules_its_gate_does_not_reach_from_the_original() -> Result<()> {
    cordial::init_tracing();
    let scratch = tempfile::tempdir().into_diagnostic()?;
    let subscriber =
        tracing_subscriber::registry().with(MetricsLayer.with_filter(metrics_filter_with(true)));
    let (affected_few, shared_few, affected_many, shared_many) =
        tracing::subscriber::with_default(subscriber, || -> Result<_> {
            project(modules_beside_a_gate(3)?.path(), "m", false)?;
            let metrics = metrics_of(scratch.path())?;
            let few = (
                sharing_field(&metrics, "affected_modules"),
                sharing_field(&metrics, "shared_modules"),
            );
            project(modules_beside_a_gate(30)?.path(), "m", false)?;
            let metrics = metrics_of(scratch.path())?;
            let many = (
                sharing_field(&metrics, "affected_modules") - few.0,
                sharing_field(&metrics, "shared_modules") - few.1,
            );
            Ok((few.0, few.1, many.0, many.1))
        })?;
    assert_eq!(
        (affected_few, shared_few),
        (1, 3),
        "the root is affected, and the three modules beside it are the original's"
    );
    assert_eq!(
        (affected_many, shared_many),
        (1, 30),
        "and the thirty are, with the root still the only one affected"
    );
    Ok(())
}

/// A crate whose root has a gate the context flips, and modules that are or are not made of it.
fn modules_of_each_kind() -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    write_workspace(root.path(), &["m".to_owned()])?;
    write_crate(
        root.path(),
        "m",
        &[],
        "mod plain;\nmod plain_too;\nmod named;\nmod globby;\nmod mac;\nmod outer;\n#[cfg(feature = \"extra\")]\npub fn only_with_extra() -> u32 { 1 }\n",
    )?;
    let src = root.path().join("m/src");
    std::fs::create_dir_all(src.join("outer")).into_diagnostic()?;
    for (file, text) in [
        ("plain.rs", "pub struct P;\n"),
        (
            "plain_too.rs",
            "use crate::plain::P;\npub fn q(_: P) -> u32 { 1 }\n",
        ),
        (
            "named.rs",
            "use crate::only_with_extra;\npub fn f() -> u32 { only_with_extra() }\n",
        ),
        ("globby.rs", "use crate::*;\npub struct G;\n"),
        ("mac.rs", "macro_rules! m { () => {}; }\npub struct M;\n"),
        ("outer/mod.rs", "mod inner;\npub struct O;\n"),
        (
            "outer/inner.rs",
            "use crate::only_with_extra;\npub fn g() -> u32 { only_with_extra() }\n",
        ),
    ] {
        std::fs::write(src.join(file), text).into_diagnostic()?;
    }
    Ok(root)
}

/// What makes a module the copy's own: an import of a name the gate defines, a glob import, a macro it defines,
/// and being the parent of one of those. The root is the copy's. What none of them reaches is the original's, and
/// that includes a module that imports from one of those.
#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_module_that_a_flipped_gate_can_reach_is_the_copys_own() -> Result<()> {
    cordial::init_tracing();
    let scratch = tempfile::tempdir().into_diagnostic()?;
    let subscriber =
        tracing_subscriber::registry().with(MetricsLayer.with_filter(metrics_filter_with(true)));
    let (affected, shared) = tracing::subscriber::with_default(subscriber, || -> Result<_> {
        project(modules_of_each_kind()?.path(), "m", false)?;
        let metrics = metrics_of(scratch.path())?;
        Ok((
            sharing_field(&metrics, "affected_modules"),
            sharing_field(&metrics, "shared_modules"),
        ))
    })?;
    // Affected: the root, `named`, `globby`, `mac`, `outer` (its child `inner` is) and `inner`. Shared: `plain`
    // and `plain_too`, which imports from it.
    assert_eq!((affected, shared), (6, 2), "affected and shared modules");
    Ok(())
}

/// How many of the functions of a context's copy are the ordinary build's own, and how many are the copy's.
fn function_sharing(root: &std::path::Path, share: bool) -> Result<(usize, usize)> {
    let options = SemanticCargoOptions::default().with_do_not_share_definition_maps(!share);
    let mut session = SourceDatabase::load_with(root, &options).into_diagnostic()?;
    let dir = root.join("m");
    session
        .project(&CrateTarget::new("m", &dir), &[dir.join("src/lib.rs")])
        .into_diagnostic()?;
    let context = cordial::SemanticContextKey::enabling(["feature=\"extra\""]);
    let added = session.add_context_crates(&context).into_diagnostic()?;
    Ok(session.context_function_sharing(&added))
}

/// A function in a module the context cannot change is the ordinary build's function in the copy too (the same
/// id), so what is computed for it is computed once: the copy's own functions are only those the gate reaches,
/// however many modules there are.
#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_function_in_a_module_the_gate_does_not_reach_is_the_original_s_in_the_copy() -> Result<()> {
    cordial::init_tracing();
    let few = function_sharing(modules_beside_a_gate(3)?.path(), true)?;
    let many = function_sharing(modules_beside_a_gate(30)?.path(), true)?;
    // With it off, every function of the copy is the copy's own.
    let off = function_sharing(modules_beside_a_gate(3)?.path(), false)?;
    assert_eq!(off, (0, 7), "(shared, own) functions with sharing off");
    // Each module has a method and a function. The gated function in the root is the copy's own.
    assert_eq!(few, (6, 1), "(shared, own) functions with 3 modules");
    assert_eq!(many, (60, 1), "(shared, own) functions with 30 modules");
    Ok(())
}

/// What a context finds does not depend on whether the modules it cannot change are the original's.
#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn what_a_context_finds_is_the_same_whether_or_not_it_takes_modules_from_the_original() -> Result<()>
{
    cordial::init_tracing();
    for (root, target) in [
        (modules_of_each_kind()?, "m"),
        (modules_beside_a_gate(6)?, "m"),
        (nested()?, "both"),
        (chain()?, "top"),
        (private_across_modules()?, "m"),
    ] {
        let found = |share: bool| -> Result<String> {
            let options = SemanticCargoOptions::default().with_do_not_share_definition_maps(!share);
            let mut session = SourceDatabase::load_with(root.path(), &options).into_diagnostic()?;
            let dir = root.path().join(target);
            let graph = session
                .project(&CrateTarget::new(target, &dir), &[dir.join("src/lib.rs")])
                .into_diagnostic()?;
            Ok(format!("{graph:?}"))
        };
        let (shared, alone) = (found(true)?, found(false)?);
        assert_eq!(
            described(&shared),
            described(&alone),
            "the graph of {target}"
        );
    }
    Ok(())
}

/// A module the gate reaches that calls a `pub(crate)` function of one it does not.
fn private_across_modules() -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    write_workspace(root.path(), &["m".to_owned()])?;
    write_crate(
        root.path(),
        "m",
        &[],
        "mod shared;\nmod uses;\n#[cfg(feature = \"extra\")]\npub fn only_with_extra() -> u32 { 1 }\n",
    )?;
    let src = root.path().join("m/src");
    std::fs::write(
        src.join("shared.rs"),
        "pub(crate) fn helper() -> u32 { 1 }\npub(super) fn nearby() -> u32 { 2 }\npub struct Shared;\nimpl Shared { pub(crate) fn method(&self) -> u32 { 3 } }\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        src.join("uses.rs"),
        "use crate::shared::Shared;\npub fn call() -> u32 { crate::shared::helper() + crate::shared::nearby() + Shared.method() + crate::only_with_extra() }\n",
    )
    .into_diagnostic()?;
    Ok(root)
}

/// What is taken from the original is not collected: a copy's map holds the modules the gate reaches and is given the
/// others, however many there are.
#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_copy_collects_none_of_the_modules_it_takes_from_the_original() -> Result<()> {
    cordial::init_tracing();
    let scratch = tempfile::tempdir().into_diagnostic()?;
    let subscriber =
        tracing_subscriber::registry().with(MetricsLayer.with_filter(metrics_filter_with(true)));
    let (few, many) = tracing::subscriber::with_default(subscriber, || -> Result<_> {
        project(modules_beside_a_gate(3)?.path(), "m", false)?;
        let few = sharing_field(&metrics_of(scratch.path())?, "taken_from_original");
        project(modules_beside_a_gate(30)?.path(), "m", false)?;
        let many = sharing_field(&metrics_of(scratch.path())?, "taken_from_original") - few;
        Ok((few, many))
    })?;
    assert_eq!(
        (few, many),
        (3, 30),
        "modules the copy was given and did not collect"
    );
    Ok(())
}

/// What a context gives up when it is done is what it made. The copy's map holds the original's modules it was
/// given, and the keys of their items and traits and impls are the ordinary build's: dropping the values keyed by
/// them would drop what the ordinary build computed.
#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_context_retires_nothing_that_is_the_ordinary_builds() -> Result<()> {
    cordial::init_tracing();
    let root = modules_beside_a_gate(5)?;
    let mut session = SourceDatabase::load_with(root.path(), &SemanticCargoOptions::default())
        .into_diagnostic()?;
    let dir = root.path().join("m");
    session
        .project(&CrateTarget::new("m", &dir), &[dir.join("src/lib.rs")])
        .into_diagnostic()?;
    let context = cordial::SemanticContextKey::enabling(["feature=\"extra\""]);
    let added = session.add_context_crates(&context).into_diagnostic()?;
    let (overlap, dropped) = session.context_retirement_overlap(&added);
    assert!(dropped > 0, "a context drops what it made");
    assert_eq!(
        overlap, 0,
        "{overlap} of the {dropped} keys it drops are the ordinary build's"
    );
    Ok(())
}

/// The events say why a module is the copy's own, so that a run on a large workspace can be read for it.
#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn the_events_say_why_each_module_is_the_copys_own() -> Result<()> {
    cordial::init_tracing();
    let scratch = tempfile::tempdir().into_diagnostic()?;
    let subscriber =
        tracing_subscriber::registry().with(MetricsLayer.with_filter(metrics_filter_with(true)));
    let (modules, files) = tracing::subscriber::with_default(subscriber, || -> Result<_> {
        project(modules_of_each_kind()?.path(), "m", false)?;
        let metrics = metrics_of(scratch.path())?;
        let of = |name: &str| {
            metrics
                .values()
                .iter()
                .filter(|(key, _)| key.ends_with(name))
                .map(|(_, count)| *count)
                .sum::<u64>()
        };
        Ok((
            [
                of("def_map.affected_by_file"),
                of("def_map.affected_by_macro"),
                of("def_map.affected_by_import"),
                of("def_map.affected_by_parent"),
            ],
            [
                of("contexts.affected_copied"),
                of("contexts.affected_by_name"),
                of("contexts.affected_by_glob"),
            ],
        ))
    })?;
    // By file: the root, `named`, `globby` and `inner`. By macro: `mac`. By import: none. By parent: `outer`.
    assert_eq!(
        modules,
        [4, 1, 0, 1],
        "modules affected by file, macro, import, parent"
    );
    // Of the files: the root is copied; `named` and `inner` mention the name; `globby` has a glob.
    assert_eq!(
        files,
        [1, 2, 1],
        "files affected by being copied, by name, by glob"
    );
    Ok(())
}

/// A module that defines a macro is the copy's own even when nothing the gate switches reaches it, and so is the
/// module above it: a `macro_rules!` is in the textual macro scope and not among a module's declarations, and
/// what is exported reaches the crate root.
#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_module_that_defines_a_macro_is_the_copys_own_whatever_is_above_it() -> Result<()> {
    cordial::init_tracing();
    let scratch = tempfile::tempdir().into_diagnostic()?;
    let subscriber =
        tracing_subscriber::registry().with(MetricsLayer.with_filter(metrics_filter_with(true)));
    let counts = tracing::subscriber::with_default(subscriber, || -> Result<_> {
        let root = tempfile::tempdir().into_diagnostic()?;
        write_workspace(root.path(), &["m".to_owned()])?;
        write_crate(
            root.path(),
            "m",
            &[],
            "mod holder;\nmod plain;\n#[cfg(feature = \"extra\")]\npub fn only_with_extra() -> u32 { 1 }\n",
        )?;
        let src = root.path().join("m/src");
        std::fs::create_dir_all(src.join("holder")).into_diagnostic()?;
        std::fs::write(src.join("holder/mod.rs"), "mod defines;\npub struct H;\n")
            .into_diagnostic()?;
        std::fs::write(
            src.join("holder/defines.rs"),
            "macro_rules! made { () => { 1 }; }\npub fn f() -> u32 { made!() }\n",
        )
        .into_diagnostic()?;
        std::fs::write(src.join("plain.rs"), "pub struct P;\n").into_diagnostic()?;
        project(root.path(), "m", false)?;
        let metrics = metrics_of(scratch.path())?;
        Ok([
            sharing_field(&metrics, "affected_by_macro"),
            sharing_field(&metrics, "affected_by_parent"),
            sharing_field(&metrics, "shared_modules"),
        ])
    })?;
    assert_eq!(
        counts,
        [1, 1, 1],
        "affected by macro, by parent, and shared: only `plain` is shared"
    );
    Ok(())
}

/// What makes a module the copy's own is a name its code uses that a flipped gate defines. A gated `use` defines
/// the names it binds and not the path to them, and a comment or a string that says the name is not a use of it.
#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_path_a_gated_use_goes_through_and_a_comment_that_names_a_gate_do_not_reach_a_module()
-> Result<()> {
    cordial::init_tracing();
    let scratch = tempfile::tempdir().into_diagnostic()?;
    let subscriber =
        tracing_subscriber::registry().with(MetricsLayer.with_filter(metrics_filter_with(true)));
    let (affected, shared) = tracing::subscriber::with_default(subscriber, || -> Result<_> {
        let root = tempfile::tempdir().into_diagnostic()?;
        write_workspace(root.path(), &["m".to_owned()])?;
        write_crate(
            root.path(),
            "m",
            &[],
            "mod plain;\nmod commented;\n#[cfg(feature = \"extra\")]\nuse std::fmt::Display;\n#[cfg(feature = \"extra\")]\npub fn only_with_extra() -> u32 { 1 }\n",
        )?;
        let src = root.path().join("m/src");
        std::fs::write(
            src.join("plain.rs"),
            "pub fn f() -> usize { std::mem::size_of::<u8>() + core::mem::size_of::<u16>() }\n",
        )
        .into_diagnostic()?;
        std::fs::write(
            src.join("commented.rs"),
            "/// `only_with_extra` and `Display` are gated in the root.\npub fn g() -> &'static str { \"only_with_extra\" }\n",
        )
        .into_diagnostic()?;
        project(root.path(), "m", false)?;
        let metrics = metrics_of(scratch.path())?;
        Ok((
            sharing_field(&metrics, "affected_modules"),
            sharing_field(&metrics, "shared_modules"),
        ))
    })?;
    assert_eq!(
        (affected, shared),
        (1, 2),
        "only the root is the copy's own: affected and shared modules"
    );
    Ok(())
}

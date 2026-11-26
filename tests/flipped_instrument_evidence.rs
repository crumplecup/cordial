//! A function in a module that only a context has, whose `#[tracing::instrument]` is written
//! `cfg_attr(not(kani), ...)`, is instrumented in the tracing inventory whichever way the context is read.

use cordial::{
    CfgContextMode, CfgPlanning, CrateTarget, IrView, SemanticCargoOptions, SemanticContextKey, SourceDatabase,
};
use miette::{IntoDiagnostic, Result};
use std::collections::BTreeMap;
use std::sync::Mutex;

static SERIAL: Mutex<()> = Mutex::new(());

const ATTRIBUTES: &str = r#"
extern crate proc_macro;
use proc_macro::TokenStream;

#[proc_macro_attribute]
pub fn instrument(_arguments: TokenStream, item: TokenStream) -> TokenStream {
    item.to_string().replacen('{', "{ let _span = (); ", 1).parse().expect("valid items")
}
"#;

const APP: &str = "\
#[cfg(feature = \"verus\")]
mod verus_render;

#[cfg(feature = \"verus\")]
#[cfg_attr(not(kani), tracing::instrument(level = \"debug\"))]
pub fn guarded() -> usize {
    3
}

#[cfg_attr(not(kani), tracing::instrument(level = \"debug\"))]
pub fn ordinary() -> usize {
    1
}
";

/// A module only the feature has, with the same gate on its functions.
const VERUS_RENDER: &str = "\
use tracing::instrument as spanned;

#[cfg_attr(not(kani), tracing::instrument(level = \"debug\"))]
pub fn carrier() -> usize {
    2
}

#[spanned]
pub fn aliased() -> usize {
    4
}

#[lookalike::instrument]
pub fn imitation() -> usize {
    5
}
";

fn specimen() -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    for directory in ["src", "tracing_attributes/src", "tracing/src", "lookalike/src"] {
        std::fs::create_dir_all(root.path().join(directory)).into_diagnostic()?;
    }
    let write = |path: &str, text: &str| std::fs::write(root.path().join(path), text).into_diagnostic();
    write(
        "Cargo.toml",
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\
         [features]\nverus = []\n[dependencies]\ntracing = { path = \"tracing\" }\nlookalike = { path = \"lookalike\" }\n[workspace]\n",
    )?;
    write(
        "tracing_attributes/Cargo.toml",
        "[package]\nname = \"tracing_attributes\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[lib]\nproc-macro = true\n",
    )?;
    write(
        "tracing/Cargo.toml",
        "[package]\nname = \"tracing\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\
         [dependencies]\ntracing_attributes = { path = \"../tracing_attributes\" }\n",
    )?;
    write(
        "lookalike/Cargo.toml",
        "[package]\nname = \"lookalike\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[lib]\nproc-macro = true\n",
    )?;
    write("lookalike/src/lib.rs", ATTRIBUTES)?;
    write("tracing_attributes/src/lib.rs", ATTRIBUTES)?;
    write("tracing/src/lib.rs", "pub use tracing_attributes::instrument;\n")?;
    write("src/lib.rs", APP)?;
    write("src/verus_render.rs", VERUS_RENDER)?;
    Ok(root)
}

/// The functions of the tracing inventory of the crate, by path, and whether each is instrumented.
fn inventory(root: &std::path::Path, mode: CfgContextMode) -> Result<BTreeMap<String, bool>> {
    let options = SemanticCargoOptions::default()
        .with_cfg_contexts(vec![
            SemanticContextKey::enabling(["kani"]),
            SemanticContextKey::enabling(["feature=verus"]),
        ])
        .with_cfg_planning(CfgPlanning::Explicit)
        .with_cfg_context_mode(mode);
    let mut session = SourceDatabase::load_with(root, &options).into_diagnostic()?;
    let graph = session
        .project(&CrateTarget::new("app", root), &[root.join("src/lib.rs"), root.join("src/verus_render.rs")])
        .into_diagnostic()?;
    let mut functions = BTreeMap::new();
    for node in graph.nodes_matching(&cordial::BasicQuery::all_nodes()) {
        let Some(found) = graph.source_tracing_inventory(node.id()).into_diagnostic()? else {
            continue;
        };
        for function in found.functions() {
            functions.insert(function.path().clone(), function.instrumented());
        }
    }
    Ok(functions)
}

/// The inventory says what the source says of the functions that only a context has, when contexts are flipped: a
/// function is instrumented when its attribute names `tracing`'s `instrument`, however it is written, and not when
/// it names another crate's macro of the same name.
#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn what_the_inventory_says_of_functions_only_a_context_has_is_what_their_attributes_name_when_contexts_are_flipped()
-> Result<()> {
    let _serial = SERIAL.lock().unwrap();
    cordial::init_tracing();
    let root = specimen()?;
    let found = inventory(root.path(), CfgContextMode::Flips)?;
    assert_eq!(found.get("ordinary"), Some(&true), "not vacuous: {found:?}");
    assert_eq!(found.get("carrier"), Some(&true), "written as `tracing::instrument` in a module only a context has: {found:?}");
    assert_eq!(found.get("aliased"), Some(&true), "an alias in a module only a context has: {found:?}");
    assert_eq!(found.get("imitation"), Some(&false), "another crate's `instrument` is not `tracing`'s: {found:?}");
    assert_eq!(found.get("guarded"), Some(&true), "behind a guard only a context turns on, in a module the ordinary build has: {found:?}");
    Ok(())
}

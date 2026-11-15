//! A function a macro generated is found again in the next process, so a restored run does not
//! evaluate it again: a derive's methods, a function an attribute macro wrapped, one two
//! attribute macros wrapped, and one a `macro_rules!` call defined.

use cordial::{CrateTarget, SavedState, SemanticCargoOptions, SourceDatabase};
use miette::{IntoDiagnostic, Result};
use std::path::Path;

const MACROS: &str = r#"
extern crate proc_macro;
use proc_macro::TokenStream;

/// Puts a statement at the start of the function's body, as `tracing::instrument` does.
#[proc_macro_attribute]
pub fn wrap(_arguments: TokenStream, item: TokenStream) -> TokenStream {
    item.to_string().replacen('{', "{ let _guard = (); ", 1).parse().expect("valid items")
}

/// A getter for each field of a struct whose fields are all `u8`, as `derive_getters` writes.
#[proc_macro_derive(Accessors)]
pub fn accessors(input: TokenStream) -> TokenStream {
    let text = input.to_string();
    let after = text.split("struct ").nth(1).expect("a struct");
    let name = after.split(|c: char| !c.is_alphanumeric() && c != '_').next().expect("a name");
    let body = after.split('{').nth(1).expect("fields").split('}').next().expect("fields");
    let getters: String = body
        .split(',')
        .filter_map(|field| field.split(':').next())
        .map(str::trim)
        .filter(|field| !field.is_empty())
        .map(|field| format!("pub fn get_{field}(&self) -> u8 {{ self.{field} }}"))
        .collect();
    format!("impl {name} {{ {getters} }}").parse().expect("valid items")
}
"#;

const USER: &str = "\
use macros::{Accessors, wrap};

#[derive(Clone, Debug, Accessors)]
pub struct Widget {
    a: u8,
    b: u8,
}

#[wrap]
pub fn once() {}

#[wrap]
#[wrap]
pub fn twice() {}

macro_rules! make {
    ($name:ident) => {
        pub fn $name() {}
    };
}
make!(generated_one);
make!(generated_two);

pub fn plain(widget: &Widget) -> u8 {
    once();
    twice();
    generated_one();
    generated_two();
    widget.get_a() + widget.get_b()
}
";

fn specimen() -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    for directory in ["src", "macros/src"] {
        std::fs::create_dir_all(root.path().join(directory)).into_diagnostic()?;
    }
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname = \"user\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\
         [dependencies]\nmacros = { path = \"macros\" }\n[workspace]\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        root.path().join("macros/Cargo.toml"),
        "[package]\nname = \"macros\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\
         [lib]\nproc-macro = true\n",
    )
    .into_diagnostic()?;
    std::fs::write(root.path().join("macros/src/lib.rs"), MACROS).into_diagnostic()?;
    std::fs::write(root.path().join("src/lib.rs"), USER).into_diagnostic()?;
    Ok(root)
}

fn run(root: &Path, saved: &SavedState) -> Result<bool> {
    let mut session =
        SourceDatabase::load_saved(root, &SemanticCargoOptions::default(), Some(saved.clone()))
            .into_diagnostic()?;
    let restored = session.restored();
    session
        .project(&CrateTarget::new("user", root), &[root.join("src/lib.rs")])
        .into_diagnostic()?;
    Ok(restored)
}

/// "N evaluated for M" from the run's evidence line, since the last call.
fn evaluated() -> Option<(usize, usize)> {
    cordial::host::take_notes()
        .events()
        .iter()
        .find_map(|event| {
            let rest = event.strip_prefix("function evidence (workspace): ")?;
            let mut words = rest.split_whitespace();
            let evaluated = words.next()?.parse().ok()?;
            let total = words.nth(2)?.parse().ok()?;
            Some((evaluated, total))
        })
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_restored_run_does_not_evaluate_the_functions_macros_generated() -> Result<()> {
    cordial::init_tracing();
    let root = specimen()?;
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = SavedState::at(state.path().join("analysis-state"));
    let _ = evaluated();

    assert!(!run(root.path(), &saved)?);
    let (cold, functions) =
        evaluated().ok_or_else(|| miette::miette!("the cold run logs its evaluation"))?;
    assert_eq!(cold, functions, "a cold run evaluates every function");
    assert!(
        functions >= 10,
        "the specimen has the plain function, the wrapped ones, the generated ones and a \
         derive's: {functions}"
    );

    assert!(
        run(root.path(), &saved)?,
        "the second run starts from the first's state"
    );
    let (again, total) =
        evaluated().ok_or_else(|| miette::miette!("the restored run logs its evaluation"))?;
    assert_eq!(total, functions);
    assert_eq!(
        again, 0,
        "{again} of {total} functions were evaluated again in the restored run"
    );
    Ok(())
}

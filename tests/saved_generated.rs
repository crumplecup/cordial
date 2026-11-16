//! A function a macro generated is found again in the next process, so a restored run does not
//! evaluate it again: a derive's methods, a function an attribute macro wrapped, one two
//! attribute macros wrapped, and one a `macro_rules!` call defined.

mod support {
    pub mod executions;
}

use cordial::{CrateTarget, SavedState, SemanticCargoOptions, SourceDatabase};
use miette::{IntoDiagnostic, Result};
use std::path::Path;
use support::executions::Executions;
use tracing_subscriber::prelude::*;

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

/// `count` of each: a type with derives, a function an attribute macro wrapped, and one a
/// `macro_rules!` call defined.
fn user_of(count: usize) -> String {
    let mut text = String::from(
        "use macros::{Accessors, wrap};\nmacro_rules! make { ($name:ident) => { pub fn $name() {} }; }\n",
    );
    for n in 0..count {
        text.push_str(&format!(
            "#[derive(Clone, Debug, Accessors)]\npub struct W{n} {{ a: u8, b: u8 }}\n\
             #[wrap]\npub fn wrapped{n}() {{}}\nmake!(made{n});\n"
        ));
    }
    text
}

/// What a restored run executed over `count` of each generated kind.
fn restored_queries_over(count: usize) -> Result<std::collections::BTreeMap<String, usize>> {
    let executions = Executions::default();
    let subscriber = tracing_subscriber::registry().with(executions.clone());
    let _subscriber = tracing::subscriber::set_default(subscriber);
    let root = specimen()?;
    std::fs::write(root.path().join("src/lib.rs"), user_of(count)).into_diagnostic()?;
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = SavedState::at(state.path().join("analysis-state"));
    run(root.path(), &saved)?;
    executions.take();
    run(root.path(), &saved)?;
    Ok(executions.take())
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn the_queries_of_a_restored_run_grow_slowly_with_the_code_macros_generated() -> Result<()> {
    cordial::init_tracing();
    let few = restored_queries_over(5)?;
    let many = restored_queries_over(45)?;
    let total = |counts: &std::collections::BTreeMap<String, usize>| counts.values().sum::<usize>();
    let mut grown: Vec<(String, usize)> = many
        .iter()
        .map(|(name, count)| {
            (
                name.clone(),
                count.saturating_sub(few.get(name).copied().unwrap_or(0)),
            )
        })
        .filter(|(_, grew)| *grew > 100)
        .collect();
    grown.sort_by_key(|(_, grew)| std::cmp::Reverse(*grew));
    grown.truncate(10);
    // Measured: about 8 queries for each generated item (a derive, an attribute macro's function,
    // a `macro_rules!` call) over a fixed cost of about 52,000, most of it loading the sysroot and
    // the proc-macro crate. Work in proportion to what the macros expand to would be far above.
    let per_item = total(&many).saturating_sub(total(&few)) / 120;
    assert!(
        per_item < 50,
        "a restored run executes {per_item} queries for each generated item: {grown:#?}"
    );
    Ok(())
}

/// `count` functions, each with `stack` attribute macros on it and `calls` macro calls in its
/// body, one of which expands to two more.
fn bodies_of(count: usize, stack: usize, calls: usize) -> String {
    let mut text = String::from(
        "use macros::wrap;\n\
         macro_rules! note { ($($t:tt)*) => { let _ = stringify!($($t)*); }; }\n\
         macro_rules! pair { ($e:expr) => { note!($e); note!($e + 1); }; }\n",
    );
    for n in 0..count {
        for _ in 0..stack {
            text.push_str("#[wrap]\n");
        }
        text.push_str(&format!("pub fn body{n}() {{\n"));
        for call in 0..calls {
            text.push_str(&format!("    pair!({call});\n"));
        }
        text.push_str("}\n");
    }
    text
}

/// What a restored run executed over `count` functions shaped by `stack` and `calls`.
fn restored_total_over(count: usize, stack: usize, calls: usize) -> Result<usize> {
    let executions = Executions::default();
    let subscriber = tracing_subscriber::registry().with(executions.clone());
    let _subscriber = tracing::subscriber::set_default(subscriber);
    let root = specimen()?;
    std::fs::write(
        root.path().join("src/lib.rs"),
        bodies_of(count, stack, calls),
    )
    .into_diagnostic()?;
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = SavedState::at(state.path().join("analysis-state"));
    run(root.path(), &saved)?;
    executions.take();
    run(root.path(), &saved)?;
    Ok(executions.take().values().sum())
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn the_queries_of_a_restored_run_by_the_macros_in_and_on_each_function() -> Result<()> {
    cordial::init_tracing();
    let per_function = |stack, calls| -> Result<usize> {
        let few = restored_total_over(5, stack, calls)?;
        let many = restored_total_over(45, stack, calls)?;
        Ok(many.saturating_sub(few) / 40)
    };
    let plain = per_function(0, 0)?;
    let with_calls = per_function(0, 6)?;
    let one = per_function(1, 0)?;
    let three = per_function(3, 0)?;
    // Measured: 1 query for a plain function and the same with six macro calls in its body (a
    // body is not expanded to name a function), 6 with one attribute macro and 16 with three:
    // about 5 for each attribute macro, which is expanded again in every process.
    assert!(
        with_calls <= plain + 2,
        "macro calls in a body cost {with_calls} against {plain} per function"
    );
    let per_attribute = three.saturating_sub(plain) / 3;
    assert!(
        (3..=12).contains(&per_attribute) && one > plain,
        "each attribute macro costs {per_attribute} queries per function (1: {one}, 3: {three})"
    );
    Ok(())
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn two_types_deriving_the_same_macro_in_one_file_are_each_found_again() -> Result<()> {
    cordial::init_tracing();
    let root = specimen()?;
    std::fs::write(
        root.path().join("src/lib.rs"),
        "use macros::Accessors;\n\
         #[derive(Clone, Accessors)]\npub struct First { a: u8, b: u8 }\n\
         #[derive(Debug, Accessors)]\npub struct Second { a: u8, b: u8 }\n\
         #[derive(Accessors)]\n#[derive(Clone)]\npub struct Third { a: u8 }\n",
    )
    .into_diagnostic()?;
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = SavedState::at(state.path().join("analysis-state"));
    assert!(!run(root.path(), &saved)?);
    let (cold, total) =
        evaluated().ok_or_else(|| miette::miette!("the cold run logs its evidence"))?;
    assert_eq!(cold, total);

    assert!(run(root.path(), &saved)?);
    let (again, total) =
        evaluated().ok_or_else(|| miette::miette!("the restored run logs its evidence"))?;
    assert_eq!(
        again, 0,
        "{again} of {total} functions were evaluated again"
    );
    Ok(())
}

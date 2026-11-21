//! Getter collection must not copy every visible binding for every expression.
#![cfg(all(feature = "hotpath-alloc", feature = "internal_error_chain"))]

use cordial::{CrateTarget, IrView, SourceDatabase};
use serde_json::{Value, json};
use std::fmt::Write;
use std::path::Path;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn source(bindings: usize, expressions: usize) -> Result<String> {
    let mut source = String::from(
        "pub struct Thing;\nimpl Thing { pub fn file(&self) -> usize { 0 } }\n\
         pub fn exercise(value: &Thing) {\n",
    );
    for binding in 0..bindings {
        writeln!(source, "let binding_{binding} = value;")?;
    }
    for _ in 0..expressions {
        writeln!(source, "let _ = binding_0.file();")?;
    }
    source.push_str("}\n");
    Ok(source)
}

#[test]
fn allocation_fixture() -> Result {
    let Some(root) = std::env::var_os("CORDIAL_GETTER_SCOPE_FIXTURE") else {
        return Ok(());
    };
    let root = Path::new(&root);
    let bindings = std::env::var("CORDIAL_GETTER_BINDINGS")?.parse()?;
    let expressions = std::env::var("CORDIAL_GETTER_EXPRESSIONS")?.parse()?;
    std::fs::create_dir(root.join("src"))?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='getter_scope'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )?;
    let source = source(bindings, expressions)?;
    let path = root.join("src/lib.rs");
    std::fs::write(&path, &source)?;
    let mut database = SourceDatabase::load(root)?;
    let mut capture =
        cordial::profiling::Capture::start(root, cordial::profiling::Identity::default())?;
    let graph = database.project(&CrateTarget::new("getter_scope", root), &[path])?;
    capture.finish(cordial::profiling::FinishReason::Normal)?;

    let facts = serde_json::to_value(graph.source_error_apply()?)?;
    let uses = facts["files"][0]["getter_uses"]["uses"]
        .as_array()
        .ok_or("getter consumers")?;
    assert_eq!(uses.len(), expressions);
    for usage in uses {
        assert_eq!(usage["kind"], "Getter");
        assert_eq!(
            usage["receiver"],
            json!({"Method": {"receiver": {"Path": ["Thing"]}, "associated": false}})
        );
        let line = usize::try_from(usage["line"].as_u64().ok_or("line")?)?;
        assert_eq!(
            source.lines().nth(line - 1),
            Some("let _ = binding_0.file();")
        );
    }
    Ok(())
}

fn allocated(bindings: usize, expressions: usize) -> Result<u64> {
    let root = tempfile::tempdir()?;
    let output = std::process::Command::new(std::env::current_exe()?)
        .args(["--exact", "allocation_fixture"])
        .env("CORDIAL_GETTER_SCOPE_FIXTURE", root.path())
        .env("CORDIAL_GETTER_BINDINGS", bindings.to_string())
        .env("CORDIAL_GETTER_EXPRESSIONS", expressions.to_string())
        .env("HOTPATH_ALLOC_CUMULATIVE", "0")
        .env("HOTPATH_ALLOC_METRIC", "bytes")
        .env("HOTPATH_METRICS_SERVER_OFF", "1")
        .env_remove("HOTPATH_OUTPUT_PATH")
        .env_remove("HOTPATH_OUTPUT_FORMAT")
        .env_remove("CORDIAL_PERF_PROFILE_DIR")
        .output()?;
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let captures = cordial::perf::read_captures(root.path())?;
    let manifest = captures[0].manifest.as_ref().ok_or("manifest")?;
    let report: Value = serde_json::from_slice(&std::fs::read(&manifest.report)?)?;
    let row = report["functions_alloc"]["data"]
        .as_array()
        .ok_or("allocation rows")?
        .iter()
        .find(|row| {
            row["name"]
                .as_str()
                .is_some_and(|name| name.ends_with("::GetterConsumerFacts::collect"))
        })
        .ok_or("getter collector row")?;
    assert_eq!(row["calls"], 1);
    eprintln!(
        "{bindings} bindings, {expressions} expressions: {}",
        row["total"]
    );
    cordial::perf::parse_bytes(row["total"].as_str().ok_or("traffic")?)
        .ok_or_else(|| "allocation bytes".into())
}

#[test]
fn expression_allocation_growth_does_not_multiply_by_visible_bindings() -> Result {
    let sparse_short = allocated(8, 16)?;
    let sparse_long = allocated(8, 256)?;
    let dense_short = allocated(128, 16)?;
    let dense_long = allocated(128, 256)?;
    let sparse_growth = sparse_long
        .checked_sub(sparse_short)
        .ok_or("sparse growth")?;
    let dense_growth = dense_long.checked_sub(dense_short).ok_or("dense growth")?;
    assert!(sparse_growth > 0);
    // Increasing visible bindings sixteen-fold must not multiply the cost of
    // adding the same expressions. Allow upstream rounding and incidental work.
    assert!(
        dense_growth < sparse_growth * 2,
        "expression traffic: sparse {sparse_growth}, dense {dense_growth}"
    );
    Ok(())
}

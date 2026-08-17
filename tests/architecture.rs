//! Architecture guards for IR enrichment invariants.

use miette::{IntoDiagnostic, WrapErr};
use std::path::{Path, PathBuf};

const NEEDLE: &str = "parse_rustdoc_json";

const SRC_ALLOWLIST: &[&str] = &[
    "src/rustdoc/inventory.rs",
    "src/rustdoc/elicit_complete.rs",
    "src/rustdoc_loader.rs",
    "src/ir/crate_load.rs",
    "src/feature_probe/mod.rs",
    "src/framework_std/match_impl.rs",
    "src/testing/",
    "src/enricher/shadow.rs",
];

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

fn path_allowed(relative: &str) -> bool {
    SRC_ALLOWLIST
        .iter()
        .any(|allowed| relative == *allowed || relative.starts_with(allowed))
}

fn file_uses_parse_rustdoc(body: &str) -> bool {
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("//")
            || trimmed.starts_with("///")
            || trimmed.starts_with('*')
            || trimmed.starts_with("pub use")
        {
            continue;
        }
        if trimmed.contains(&format!("{NEEDLE}("))
            || (trimmed.starts_with("use ") && trimmed.contains(NEEDLE))
        {
            return true;
        }
    }
    false
}

#[test]
fn parse_rustdoc_json_confined_to_allowlist_in_src() -> miette::Result<()> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let src_root = manifest_dir.join("src");
    let mut files = Vec::new();
    collect_rs_files(&src_root, &mut files);

    let mut violations = Vec::new();
    for path in files {
        let relative = path
            .strip_prefix(&manifest_dir)
            .into_diagnostic()
            .wrap_err("under manifest")?
            .to_string_lossy()
            .replace('\\', "/");
        if path_allowed(&relative) {
            continue;
        }
        let body = std::fs::read_to_string(&path)
            .into_diagnostic()
            .wrap_err("read source")?;
        if file_uses_parse_rustdoc(&body) {
            violations.push(relative);
        }
    }

    violations.sort();
    assert!(
        violations.is_empty(),
        "parse_rustdoc_json must stay on the loader/oracle allowlist; found in:\n{}",
        violations.join("\n")
    );
    Ok(())
}

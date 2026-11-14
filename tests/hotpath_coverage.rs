//! Every function instrumented at `debug` or `info` is measured by hotpath too.
//!
//! The levels already say what matters: `trace` marks small helpers, `debug`
//! and `info` mark operations. Measuring each operation as it is written means a
//! profile of any run shows every stage and where it spends its time, without
//! adding measures after a run has already been slow.

use std::path::Path;

/// Functions that are not measured: a `Drop` impl can run while thread-local
/// storage is being torn down, which hotpath's recorder cannot survive.
const NOT_MEASURED: &[&str] = &["drop"];

fn sources() -> Vec<(String, Vec<String>)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    walkdir::WalkDir::new(&root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "rs"))
        .filter_map(|entry| {
            let text = std::fs::read_to_string(entry.path()).ok()?;
            let name = entry.path().strip_prefix(&root).ok()?.display().to_string();
            Some((name, text.lines().map(str::to_owned).collect()))
        })
        .collect()
}

/// The line where the attribute starting at `start` ends.
fn attribute_end(lines: &[String], start: usize) -> usize {
    let mut depth = 0_i32;
    for (offset, line) in lines[start..].iter().enumerate() {
        for c in line.chars() {
            match c {
                '[' => depth += 1,
                ']' => depth -= 1,
                _ => {}
            }
        }
        if depth <= 0 {
            return start + offset;
        }
    }
    start
}

fn level_of(attribute: &str) -> &str {
    attribute
        .split("level")
        .nth(1)
        .and_then(|rest| rest.split('"').nth(1))
        .unwrap_or("info")
}

#[test]
fn every_operation_is_measured() {
    cordial::init_tracing();
    let mut missing = Vec::new();
    for (file, lines) in sources() {
        let mut index = 0;
        while index < lines.len() {
            let trimmed = lines[index].trim_start();
            if !(trimmed.starts_with("#[instrument")
                || trimmed.starts_with("#[tracing::instrument"))
            {
                index += 1;
                continue;
            }
            let end = attribute_end(&lines, index);
            let level = level_of(&lines[index..=end].join(" ")).to_owned();
            // The attributes before and after this one, then the function itself.
            let mut block = lines[index..=end].join("\n");
            let mut before = index;
            while before > 0
                && (lines[before - 1].trim_start().starts_with("#[")
                    || lines[before - 1].trim_start().starts_with("///"))
            {
                before -= 1;
                block.push_str(&lines[before]);
            }
            let mut after = end + 1;
            while after < lines.len() && lines[after].trim_start().starts_with("#[") {
                let attribute_last = attribute_end(&lines, after);
                block.push_str(&lines[after..=attribute_last].join("\n"));
                after = attribute_last + 1;
            }
            let function = lines.get(after).map(String::as_str).unwrap_or_default();
            let name = function.split("fn ").nth(1).map(|rest| {
                rest.chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect::<String>()
            });
            if let Some(name) = name
                && matches!(level.as_str(), "debug" | "info")
                && !NOT_MEASURED.contains(&name.as_str())
                && !block.contains("hotpath::measure")
            {
                missing.push(format!("{file}:{}: fn {name}", index + 1));
            }
            index = after.max(index + 1);
        }
    }
    assert!(
        missing.is_empty(),
        "{} instrumented functions have no `#[hotpath::measure]`:\n{}",
        missing.len(),
        missing
            .iter()
            .take(40)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

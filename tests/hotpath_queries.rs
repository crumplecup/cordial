//! Every Salsa query reports its reuse in a profile without a counter of its own.
//!
//! A tracked function is measured, and its execution reaches a measured region,
//! possibly through local untracked helpers. The outer region counts requests;
//! the execution path is entered only when Salsa computes the result. Shared
//! callees aggregate executions; their counts do not distinguish each caller.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

fn sources() -> Vec<(String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    walkdir::WalkDir::new(&root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "rs"))
        .filter_map(|entry| {
            let text = std::fs::read_to_string(entry.path()).ok()?;
            let name = entry.path().strip_prefix(&root).ok()?.display().to_string();
            Some((name, text))
        })
        .collect()
}

/// The name of the function declared at or after `from`, and where its body is.
fn function_at(text: &str, from: usize) -> Option<(String, std::ops::Range<usize>)> {
    let declared = from + text[from..].find("fn ")?;
    let name: String = text[declared + 3..]
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    let open = declared + text[declared..].find('{')?;
    let mut depth = 0_i32;
    for (offset, c) in text[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some((name, open..open + offset));
                }
            }
            _ => {}
        }
    }
    None
}

/// Whether an attribute block (the lines before the `fn` at `declared`) holds `needle`.
fn attributes_before(text: &str, declared: usize, needle: &str) -> bool {
    let start = text[..declared].rfind("\n\n").map_or(0, |at| at + 2);
    text[start..declared].contains(needle)
}

/// Forward through ordinary local helpers; an unmeasured tracked query is a boundary.
fn untracked_helpers(text: &str) -> BTreeMap<String, &str> {
    text.match_indices("fn ")
        .filter_map(|(at, _)| {
            let (name, body) = function_at(text, at)?;
            if attributes_before(text, at, "salsa::tracked") {
                return None;
            }
            Some((name, &text[body]))
        })
        .collect()
}

fn reaches_measured_region(
    query: &str,
    body: &str,
    measured: &BTreeSet<String>,
    helpers: &BTreeMap<String, &str>,
) -> bool {
    let mut seen = BTreeSet::from([query.to_owned()]);
    let mut pending = vec![body];
    while let Some(body) = pending.pop() {
        if measured
            .iter()
            .any(|name| name != query && body.contains(&format!("{name}(")))
        {
            return true;
        }
        for (name, body_of_helper) in helpers {
            if body.contains(&format!("{name}(")) && seen.insert(name.clone()) {
                pending.push(body_of_helper);
            }
        }
    }
    false
}

#[test]
fn delegate_check_follows_helpers_but_rejects_unmeasured_paths_and_cycles() {
    let text = "
fn forward() { measured(); }

fn first() { second(); }

fn second() { first(); }

#[salsa::tracked]
fn boundary() { measured(); }
";
    let measured = BTreeSet::from(["measured".to_owned(), "query".to_owned()]);
    let helpers = untracked_helpers(text);
    for (body, expected) in [
        ("measured()", true),
        ("forward()", true),
        ("first()", false),
        ("missing()", false),
        ("query()", false),
        ("boundary()", false),
    ] {
        assert_eq!(
            reaches_measured_region("query", body, &measured, &helpers),
            expected,
            "{body}"
        );
    }
}

#[test]
fn every_tracked_query_is_measured_and_delegates_to_a_measured_function() {
    cordial::init_tracing();
    let sources = sources();
    // Every function carrying a measure, by name.
    let mut measured: BTreeSet<String> = BTreeSet::new();
    for (_, text) in &sources {
        for (at, _) in text.match_indices("#[hotpath::measure") {
            if let Some((name, _)) = function_at(text, at) {
                measured.insert(name);
            }
        }
    }
    let mut problems = Vec::new();
    let mut found = 0;
    for (file, text) in &sources {
        let helpers = untracked_helpers(text);
        for (at, _) in text.match_indices("#[salsa::tracked") {
            let Some((name, body)) = function_at(text, at) else {
                continue;
            };
            // Tracked structs are not queries.
            let declared = at + text[at..].find("fn ").unwrap_or_default();
            if text[at..declared].contains("struct ") {
                continue;
            }
            found += 1;
            let line = text[..at].matches('\n').count() + 1;
            if !attributes_before(text, declared, "hotpath::measure") {
                problems.push(format!("{file}:{line}: query {name} is not measured"));
            }
            if !reaches_measured_region(&name, &text[body], &measured, &helpers) {
                problems.push(format!(
                    "{file}:{line}: query {name} reaches no measured execution region through local untracked helpers"
                ));
            }
        }
    }
    assert!(found > 10, "found only {found} queries: the scan is blind");
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

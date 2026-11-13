//! The run-cache key digests the analysis sources and leaves out the rest. That
//! is safe only while nothing it leaves out is used by anything it keeps, so
//! this reads the sources, resolves every path they use to the module it names,
//! and fails on any that reaches into what the key ignores, or on a list that
//! has drifted from the tree.

include!("../src/run_cache/analysis_scope.rs");
include!("../src/run_cache/analysis_digest.rs");

use std::collections::BTreeSet;
use std::path::Path;

type ModulePath = Vec<String>;

fn sources() -> Vec<(String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    walkdir::WalkDir::new(&root)
        .into_iter()
        .filter_map(std::result::Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "rs"))
        .filter_map(|entry| {
            let relative = entry.path().strip_prefix(&root).ok()?.to_path_buf();
            let text = std::fs::read_to_string(entry.path()).ok()?;
            Some((relative.to_string_lossy().replace('\\', "/"), text))
        })
        .collect()
}

fn is_outside(path: &str) -> bool {
    OUTSIDE_THE_ANALYSIS
        .iter()
        .any(|entry| path == *entry || (entry.ends_with('/') && path.starts_with(entry)))
}

/// The module a source file is: `a/b.rs` is `a::b`, `a/mod.rs` and `a.rs` are `a`.
fn module_of(path: &str) -> ModulePath {
    let mut parts: Vec<String> = path
        .trim_end_matches(".rs")
        .split('/')
        .map(str::to_owned)
        .collect();
    if parts
        .last()
        .is_some_and(|last| last == "mod" || last == "lib")
    {
        parts.pop();
    }
    parts
}

/// Source with comments and string literals blanked, so a mention in prose or a
/// message is not mistaken for a use.
fn code_only(text: &str) -> Vec<char> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::with_capacity(chars.len());
    let mut index = 0;
    while index < chars.len() {
        let here = chars[index];
        let next = chars.get(index + 1).copied();
        if here == '/' && next == Some('/') {
            while index < chars.len() && chars[index] != '\n' {
                index += 1;
            }
        } else if here == '/' && next == Some('*') {
            index += 2;
            while index + 1 < chars.len() && !(chars[index] == '*' && chars[index + 1] == '/') {
                index += 1;
            }
            index += 2;
        } else if here == '"' {
            index += 1;
            while index < chars.len() && chars[index] != '"' {
                index += if chars[index] == '\\' { 2 } else { 1 };
            }
            index += 1;
            out.extend(['"', '"']);
        } else {
            out.push(here);
            index += 1;
        }
    }
    out
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn skip_space(chars: &[char], index: &mut usize) {
    while chars.get(*index).is_some_and(|c| c.is_whitespace()) {
        *index += 1;
    }
}

/// Read a path or `use` tree starting at `index`, adding every leaf path, each
/// with its alias if it has one.
fn parse_tree(
    chars: &[char],
    index: &mut usize,
    prefix: &[String],
    out: &mut Vec<(ModulePath, Option<String>)>,
) {
    skip_space(chars, index);
    if chars.get(*index) == Some(&'{') {
        *index += 1;
        loop {
            parse_tree(chars, index, prefix, out);
            skip_space(chars, index);
            match chars.get(*index) {
                Some(',') => *index += 1,
                Some('}') => {
                    *index += 1;
                    return;
                }
                _ => return,
            }
        }
    }
    let start = *index;
    while chars.get(*index).is_some_and(|c| is_ident(*c) || *c == '*') {
        *index += 1;
    }
    if *index == start {
        if !prefix.is_empty() {
            out.push((prefix.to_vec(), None));
        }
        return;
    }
    let mut path = prefix.to_vec();
    path.push(chars[start..*index].iter().collect());
    if chars.get(*index) == Some(&':') && chars.get(*index + 1) == Some(&':') {
        *index += 2;
        parse_tree(chars, index, &path, out);
        return;
    }
    let mut look = *index;
    skip_space(chars, &mut look);
    let mut alias = None;
    if chars[look..].starts_with(&['a', 's', ' ']) {
        let mut after = look + 3;
        skip_space(chars, &mut after);
        let begin = after;
        while chars.get(after).is_some_and(|c| is_ident(*c)) {
            after += 1;
        }
        if after > begin {
            alias = Some(chars[begin..after].iter().collect::<String>());
            *index = after;
        }
    }
    out.push((path, alias));
}

/// Whether the path at `index` is part of a `pub use` statement: the text since
/// the last `;`, without its attributes, begins with `pub use`.
fn in_reexport(chars: &[char], index: usize) -> bool {
    let start = chars[..index]
        .iter()
        .rposition(|c| *c == ';')
        .map_or(0, |at| at + 1);
    let mut statement = String::new();
    let mut depth = 0;
    let mut attribute = false;
    for (offset, c) in chars[start..index].iter().enumerate() {
        match c {
            '#' if chars.get(start + offset + 1) == Some(&'[') => attribute = true,
            '[' if attribute => depth += 1,
            ']' if attribute => {
                depth -= 1;
                if depth == 0 {
                    attribute = false;
                }
            }
            _ if attribute => {}
            _ => statement.push(*c),
        }
    }
    let statement = statement.trim_start();
    ["pub use ", "pub(crate) use ", "pub(super) use "]
        .iter()
        .any(|head| statement.starts_with(head))
}

/// Every path a file writes that starts `crate::`, `super::` or `self::`, and
/// every `use` tree, as written.
fn written_paths(chars: &[char]) -> Vec<(ModulePath, Option<String>)> {
    let mut found = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        if index > 0 && is_ident(chars[index - 1]) || chars[index] == '$' {
            index += 1;
            continue;
        }
        let rest: String = chars[index..chars.len().min(index + 8)].iter().collect();
        for keyword in ["crate", "super", "self"] {
            if rest.starts_with(&format!("{keyword}::")) && !in_reexport(chars, index) {
                let mut at = index + keyword.len() + 2;
                parse_tree(chars, &mut at, &[keyword.to_owned()], &mut found);
            }
        }
        let before: String = chars[index.saturating_sub(12)..index].iter().collect();
        let reexport = before.trim_end().ends_with("pub")
            || before.trim_end().ends_with("pub(crate)")
            || before.trim_end().ends_with("pub(super)");
        if rest.starts_with("use ") && !rest.starts_with("use crate") && !reexport {
            let mut at = index + 4;
            skip_space(chars, &mut at);
            parse_tree(chars, &mut at, &[], &mut found);
        }
        index += 1;
    }
    found
}

/// A written path as the absolute module path it names, from the file at `own`.
fn resolve(
    written: &[String],
    own: &[String],
    children: &BTreeSet<ModulePath>,
) -> Option<ModulePath> {
    let (first, rest) = written.split_first()?;
    let mut base: ModulePath = match first.as_str() {
        "crate" => Vec::new(),
        "self" => own.to_vec(),
        "super" => own[..own.len().saturating_sub(1)].to_vec(),
        _ => {
            // A bare path in a `use` names a child module of this one, or an
            // external crate, which is none of ours.
            let mut child = own.to_vec();
            child.push(first.clone());
            if !children.contains(&child) {
                return None;
            }
            child
        }
    };
    let mut rest = rest;
    while let Some((next, tail)) = rest.split_first() {
        if next == "super" {
            base.pop();
            rest = tail;
        } else {
            break;
        }
    }
    base.extend(rest.iter().filter(|s| *s != "self").cloned());
    Some(base)
}

fn starts_with(path: &[String], prefix: &[String]) -> bool {
    path.len() >= prefix.len() && path[..prefix.len()] == *prefix
}

#[test]
fn the_list_names_only_paths_that_exist() {
    let files: Vec<String> = sources().into_iter().map(|(path, _)| path).collect();
    for entry in OUTSIDE_THE_ANALYSIS {
        let found = files
            .iter()
            .any(|path| path == entry || (entry.ends_with('/') && path.starts_with(entry)));
        assert!(
            found,
            "{entry} is listed as outside the analysis but is not in src/"
        );
    }
}

/// Every use, by a file the key digests, of a module it ignores.
fn violations(sources: &[(String, String)], is_outside: &dyn Fn(&str) -> bool) -> BTreeSet<String> {
    let all_modules: BTreeSet<ModulePath> =
        sources.iter().map(|(path, _)| module_of(path)).collect();
    let outside: Vec<ModulePath> = sources
        .iter()
        .filter(|(path, _)| is_outside(path))
        .map(|(path, _)| module_of(path))
        .collect();

    // Names a module re-exports from a module the key ignores: a path to the
    // name is a path into it, though it does not say so.
    let mut reexported: Vec<ModulePath> = Vec::new();
    loop {
        let before = reexported.len();
        for (path, text) in sources {
            let own = module_of(path);
            let chars = code_only(text);
            let mut index = 0;
            while index + 8 < chars.len() {
                let here: String = chars[index..index + 8].iter().collect();
                if here == "pub use " && (index == 0 || !is_ident(chars[index - 1])) {
                    let mut at = index + 8;
                    let mut leaves = Vec::new();
                    parse_tree(&chars, &mut at, &[], &mut leaves);
                    for (leaf, alias) in leaves {
                        let Some(target) = resolve(&leaf, &own, &all_modules) else {
                            continue;
                        };
                        if outside.iter().any(|module| starts_with(&target, module))
                            || reexported.iter().any(|name| starts_with(&target, name))
                        {
                            let name = alias.or_else(|| leaf.last().cloned()).unwrap_or_default();
                            let mut exported = own.clone();
                            exported.push(name);
                            if !reexported.contains(&exported) {
                                reexported.push(exported);
                            }
                        }
                    }
                }
                index += 1;
            }
        }
        if reexported.len() == before {
            break;
        }
    }

    let mut violations = BTreeSet::new();
    for (path, text) in sources {
        if is_outside(path) {
            continue;
        }
        let own = module_of(path);
        let parent_is_outside = sources.iter().any(|(other, _)| {
            is_outside(other) && module_of(other) == own[..own.len().saturating_sub(1)]
        });
        for (written, _) in written_paths(&code_only(text)) {
            if written.first().is_some_and(|first| first == "super") && parent_is_outside {
                violations.insert(format!(
                    "{path}: uses its parent module, which the key ignores"
                ));
            }
            let Some(target) = resolve(&written, &own, &all_modules) else {
                continue;
            };
            if outside.iter().any(|module| starts_with(&target, module))
                || reexported.iter().any(|name| starts_with(&target, name))
            {
                violations.insert(format!("{path}: uses {}", target.join("::")));
            }
        }
    }
    violations
}

#[test]
fn nothing_the_key_digests_uses_what_it_leaves_out() {
    let found = violations(&sources(), &is_outside);
    assert!(
        found.is_empty(),
        "analysis sources use code the key ignores:\n{}",
        found.into_iter().collect::<Vec<_>>().join("\n")
    );
}

fn tree(files: &[(&str, &str)]) -> Vec<(String, String)> {
    files
        .iter()
        .map(|(path, text)| ((*path).to_owned(), (*text).to_owned()))
        .collect()
}

fn ignoring_ui(path: &str) -> bool {
    path.starts_with("ui/")
}

#[test]
fn the_scan_finds_every_way_in_to_what_the_key_ignores() {
    let sources = tree(&[
        (
            "lib.rs",
            "mod ui;\nmod core;\npub use ui::{Shown as Visible};\n",
        ),
        ("ui.rs", "pub mod menu;\npub use menu::Shown;\n"),
        ("ui/menu.rs", "pub struct Shown;\npub fn draw() {}\n"),
        (
            "core.rs",
            "mod direct;\nmod braced;\nmod renamed;\nmod relative;\nmod fine;\n",
        ),
        ("core/direct.rs", "fn f() { crate::ui::menu::draw(); }\n"),
        ("core/braced.rs", "use crate::{other, ui::menu::Shown};\n"),
        ("core/renamed.rs", "use crate::Visible;\n"),
        ("core/relative.rs", "use super::super::ui::menu::draw;\n"),
        (
            "core/fine.rs",
            "// crate::ui::menu::draw is only mentioned\nfn f() { let _ = \"crate::ui::menu\"; crate::core::direct(); }\n",
        ),
    ]);
    let found = violations(&sources, &ignoring_ui);
    for file in ["direct", "braced", "renamed", "relative"] {
        assert!(
            found
                .iter()
                .any(|line| line.starts_with(&format!("core/{file}.rs"))),
            "{file} reaches the ignored module and was not found: {found:?}"
        );
    }
    assert!(
        !found.iter().any(|line| line.starts_with("core/fine.rs")),
        "a mention in a comment or a string is not a use: {found:?}"
    );
    assert!(
        !found
            .iter()
            .any(|line| line.starts_with("ui.rs") || line.starts_with("lib.rs")),
        "a parent passing a name along is not a use: {found:?}"
    );
}

#[test]
fn the_digest_moves_with_what_shapes_the_analysis_and_not_with_the_rest() -> std::io::Result<()> {
    let scratch = tempfile::tempdir()?;
    let src = scratch.path();
    for (path, text) in [
        ("lib.rs", "mod ir;\n"),
        ("ir.rs", "pub struct Node;\n"),
        ("cli.rs", "fn main_help() {}\n"),
        ("host/transport/client.rs", "fn connect() {}\n"),
        ("host/databases.rs", "fn keep() {}\n"),
    ] {
        let file = src.join(path);
        std::fs::create_dir_all(file.parent().unwrap_or(src))?;
        std::fs::write(file, text)?;
    }
    let base = analysis_sources_digest(src)?;
    assert_eq!(base, analysis_sources_digest(src)?, "it is stable");

    // The transport and the CLI are not the analysis.
    std::fs::write(src.join("cli.rs"), "fn main_help() { /* edited */ }\n")?;
    std::fs::write(
        src.join("host/transport/client.rs"),
        "fn connect_again() {}\n",
    )?;
    std::fs::write(src.join("host/transport/new_file.rs"), "fn another() {}\n")?;
    assert_eq!(
        base,
        analysis_sources_digest(src)?,
        "edits outside leave it"
    );

    // Everything else is, including the kept-database half of the same module.
    for (path, text) in [
        ("ir.rs", "pub struct Node(u8);\n"),
        ("host/databases.rs", "fn keep() { /* edited */ }\n"),
        ("lib.rs", "mod ir;\nmod more;\n"),
    ] {
        let before = analysis_sources_digest(src)?;
        std::fs::write(src.join(path), text)?;
        assert_ne!(before, analysis_sources_digest(src)?, "{path} moves it");
    }
    // A file nobody has classified is analysis.
    let before = analysis_sources_digest(src)?;
    std::fs::write(src.join("unclassified.rs"), "fn f() {}\n")?;
    assert_ne!(before, analysis_sources_digest(src)?);
    // Renaming a kept file moves it even when the bytes are the same.
    let before = analysis_sources_digest(src)?;
    std::fs::rename(src.join("unclassified.rs"), src.join("renamed.rs"))?;
    assert_ne!(before, analysis_sources_digest(src)?);
    Ok(())
}

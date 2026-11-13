//! Locks and channels come from `crate::sync`, so hotpath's lock and channel
//! sections see all of them, and each is named distinctly in a profile.

use std::collections::BTreeMap;
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

/// The error types of `std::sync::mpsc`, which the instrumented channels share
/// with the standard library and which are not channels.
const SHARED_ERRORS: &[&str] = &[
    "RecvError",
    "RecvTimeoutError",
    "SendError",
    "TryRecvError",
    "TrySendError",
];

#[test]
fn locks_and_channels_come_from_the_sync_module() {
    let mut raw = Vec::new();
    for (file, text) in sources() {
        if file == "sync.rs" {
            continue;
        }
        for (index, line) in text.lines().enumerate() {
            let code = line.split("//").next().unwrap_or_default();
            let names_std_sync = code.contains("std::sync::");
            let uses_std_sync = code.contains("use std::sync::");
            for primitive in ["Mutex", "RwLock", "mpsc"] {
                // `std::sync::Mutex`, `std::sync::mpsc::..`, or a use list that names one.
                let direct = code.contains(&format!("std::sync::{primitive}"));
                let listed = uses_std_sync && code.contains(primitive);
                if !(direct || listed) {
                    continue;
                }
                let only_errors = primitive == "mpsc"
                    && SHARED_ERRORS.iter().any(|error| code.contains(error))
                    && !code.contains("channel");
                if !only_errors {
                    raw.push(format!("{file}:{}: {}", index + 1, line.trim()));
                }
            }
            let _ = names_std_sync;
        }
    }
    assert!(
        raw.is_empty(),
        "use crate::sync for locks and channels:\n{}",
        raw.join("\n")
    );
}

#[test]
fn every_lock_and_channel_has_a_name_of_its_own() {
    let mut seen: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (file, text) in sources() {
        for (at, _) in text
            .match_indices("sync::mutex!(")
            .chain(text.match_indices("sync::channel!("))
        {
            // The invocation ends at the parenthesis that closes its own.
            let open = at + text[at..].find('(').unwrap_or_default();
            let mut depth = 0_i32;
            let mut end = text.len();
            for (offset, c) in text[open..].char_indices() {
                match c {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            end = open + offset;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            // The label is the last string literal of the invocation.
            let label = text[open..end]
                .rsplit('"')
                .nth(1)
                .unwrap_or_default()
                .to_owned();
            let line = text[..at].matches('\n').count() + 1;
            seen.entry(label)
                .or_default()
                .push(format!("{file}:{line}"));
        }
    }
    assert!(
        !seen.is_empty(),
        "no instrumented locks found: the scan is blind"
    );
    let duplicated: Vec<_> = seen
        .iter()
        .filter(|(label, sites)| label.is_empty() || sites.len() > 1)
        .collect();
    assert!(
        duplicated.is_empty(),
        "unnamed or repeated labels: {duplicated:?}"
    );
}

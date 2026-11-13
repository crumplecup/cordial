//! Every thread the library starts has a name, so hotpath's thread section says
//! what each one is (the CPU and the bytes of an unnamed thread cannot be read).

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

#[test]
fn no_thread_is_started_without_a_name() {
    let mut unnamed = Vec::new();
    for (file, text) in sources() {
        for (index, line) in text.lines().enumerate() {
            if line.contains("thread::spawn(") && !line.trim_start().starts_with("//") {
                unnamed.push(format!("{file}:{}: thread::spawn has no name", index + 1));
            }
        }
        for (at, _) in text.match_indices("thread::Builder::new()") {
            // The name is set within the builder's own chain.
            let chain: String = text[at..].chars().take(260).collect();
            let chain = chain.split(".spawn").next().unwrap_or_default();
            let line = text[..at].matches('\n').count() + 1;
            if !chain.contains(".name(") {
                unnamed.push(format!("{file}:{line}: Builder without .name(..)"));
            } else if !chain.contains("\"cordial-") && !chain.contains("format!(\"cordial-") {
                unnamed.push(format!(
                    "{file}:{line}: thread name does not start with cordial-"
                ));
            }
        }
    }
    assert!(
        unnamed.is_empty(),
        "unnamed threads:\n{}",
        unnamed.join("\n")
    );
}

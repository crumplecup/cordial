//! Receiver evidence survives lexical scope entry, shadowing and restoration.
#![cfg(feature = "internal_error_chain")]

use cordial::{CrateTarget, IrView, SourceDatabase};
use serde_json::{Value, json};

#[test]
fn getter_receivers_preserve_lexical_scope() -> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    std::fs::create_dir(root.path().join("src"))?;
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='getter_scope'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )?;
    let source = r#"
pub struct Thing;
pub struct Other;
impl Thing {
    pub fn file(&self) -> usize { 0 }
    pub fn exercise(&self, other: &Other) {
        self.file(); // Thing
        { let value = other; value.file(); } // Other
        self.file(); // Thing
    }
}
impl Other { pub fn file(&self) -> usize { 1 } }
pub fn exercise(value: &Thing, other: &Other) {
    let alias = value;
    alias.file(); // Thing
    { let value = other; value.file(); } // Other
    value.file(); // Thing
    let value = value;
    value.file(); // Thing
    let value = other;
    value.file(); // Other
    let _closure = |value: &Thing| { value.file(); }; // Unknown
    value.file(); // Other
    fn local(value: &Thing) { value.file(); } // Thing
    value.file(); // Other
    {
        type Alias = Thing;
        let typed: &Thing = todo!();
        typed.file(); // Unknown
    }
    value.file(); // Other
}
"#;
    let path = root.path().join("src/lib.rs");
    std::fs::write(&path, source)?;
    let mut database = SourceDatabase::load(root.path())?;
    let graph = database.project(&CrateTarget::new("getter_scope", root.path()), &[path])?;
    let facts = serde_json::to_value(graph.source_error_apply()?)?;
    let uses = facts["files"][0]["getter_uses"]["uses"]
        .as_array()
        .ok_or("getter consumers")?;
    let expected: Vec<_> = source
        .lines()
        .enumerate()
        .filter_map(|(line, text)| text.split_once("// ").map(|(_, kind)| (line + 1, kind)))
        .collect();
    assert_eq!(uses.len(), expected.len());
    for (usage, (line, kind)) in uses.iter().zip(expected) {
        assert_eq!(usage["line"], line);
        assert_eq!(usage["kind"], "Getter");
        let receiver = if kind == "Unknown" {
            Value::String("Unknown".to_owned())
        } else {
            json!({"Path": [kind]})
        };
        assert_eq!(
            usage["receiver"],
            json!({"Method": {"receiver": receiver, "associated": false}}),
            "line {line}"
        );
    }
    Ok(())
}

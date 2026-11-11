//! A checklist file for the tracing apply tests, in the format `cordial` writes.

pub fn checklist_for_crate(crate_name: &str, items: &[(&str, u32)]) -> String {
    let mut body =
        format!("# Tracing instrument checklist\n\n**Open gaps:** n\n\n## `{crate_name}`\n\n");
    for (qualified_name, line) in items {
        body.push_str(&format!(
            "- [ ] `{qualified_name}` — `src/lib.rs:{line}` (pub)\n"
        ));
    }
    body
}

//! A function with a body of a chosen length, and an oversize one.

pub fn large_function_fixture() -> String {
    function_with_body_lines("oversized", 202)
}

pub fn function_with_body_lines(name: &str, body_lines: u32) -> String {
    let inner = body_lines.saturating_sub(2);
    let mut body = format!("pub fn {name}() {{\n");
    for index in 0..inner {
        body.push_str(&format!("    let _ = {index};\n"));
    }
    body.push_str("}\n");
    body
}

//! A module-sized source of a chosen line count.

pub fn padded_module(lines: usize) -> String {
    let mut body = String::from("pub fn x() {}\n");
    for _ in 1..lines {
        body.push_str("// pad\n");
    }
    body
}

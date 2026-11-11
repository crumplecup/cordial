//! Pads source to a line count with comment lines.

pub fn pad_source_to_lines(mut source: String, lines: usize) -> String {
    let current = source.lines().count();
    for _ in current..lines {
        source.push_str("// pad\n");
    }
    source
}

//! Distinct native source types under one caller and binary return boundary.

fn io_operation(fail: bool) -> ::std::result::Result<u32, ::std::io::Error> {
    if fail {
        Err(::std::io::Error::from(
            ::std::io::ErrorKind::PermissionDenied,
        ))
    } else {
        Ok(7)
    }
}

fn parse_operation(input: &str) -> ::std::result::Result<u32, ::std::num::ParseIntError> {
    input.parse::<u32>()
}

fn leaf(mode: u32) -> u32 {
    let first = io_operation(mode == 1).unwrap();
    let input = if mode == 2 { "invalid" } else { "5" };
    first + parse_operation(input).unwrap()
}

fn entry(mode: u32) -> u32 {
    leaf(mode)
}

fn main() {
    let mode = match ::std::env::args().nth(1).as_deref() {
        Some("io") => 1,
        Some("parse") => 2,
        _ => 0,
    };
    let value = entry(mode);
    let output = if value == 12 { "success\n" } else { "wrong\n" };
    let _ = ::std::io::Write::write_all(&mut ::std::io::stdout(), output.as_bytes());
}

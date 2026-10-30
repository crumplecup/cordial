//! Multiple native I/O failures sharing one closed binary caller component.

fn operation(step: u32, fail: u32) -> ::std::result::Result<u32, ::std::io::Error> {
    let marker = match step {
        1 => "1\n",
        2 => "2\n",
        _ => "3\n",
    };
    let _ = ::std::io::Write::write_all(&mut ::std::io::stdout(), marker.as_bytes());
    if step == fail {
        Err(::std::io::Error::from(
            ::std::io::ErrorKind::PermissionDenied,
        ))
    } else {
        Ok(step)
    }
}

fn pair(fail: u32) -> u32 {
    let first = operation(1, fail).unwrap();
    first
        + operation(2, fail) /* preserve later receiver */
            .unwrap()
}

fn other(fail: u32) -> u32 {
    operation(3, fail).unwrap()
}

fn left(fail: u32) -> u32 {
    pair(fail)
}

fn right(fail: u32) -> u32 {
    other(fail) + pair(0)
}

fn main() {
    let fail = match ::std::env::args().nth(1).as_deref() {
        Some("first") => 1,
        Some("second") => 2,
        Some("third") => 3,
        _ => 0,
    };
    let value = left(fail) + right(fail);
    let output = if value == 9 { "success\n" } else { "wrong\n" };
    let _ = ::std::io::Write::write_all(&mut ::std::io::stdout(), output.as_bytes());
}

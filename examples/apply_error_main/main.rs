//! A fresh panic component whose binary boundary can return a typed error.

fn operation(fail: bool) -> ::std::result::Result<u32, ::std::io::Error> {
    if fail {
        Err(::std::io::Error::from(
            ::std::io::ErrorKind::PermissionDenied,
        ))
    } else {
        Ok(7)
    }
}

fn leaf(fail: bool, count: &mut u32) -> u32 {
    *count += 1;
    operation(fail) /* preserve receiver comment */
        .unwrap()
}

fn left(fail: bool, count: &mut u32) -> u32 {
    leaf(fail, count) + leaf(false, count)
}

fn right(fail: bool, count: &mut u32) -> u32 {
    leaf(fail, count)
}

fn middle(fail: bool, count: &mut u32) -> u32 {
    left(fail, count) + right(false, count)
}

fn recursive(fail: bool, depth: u32, count: &mut u32) -> u32 {
    if depth == 0 {
        middle(fail, count)
    } else if depth > 2 {
        recursive(fail, depth - 1, count) + 1
    } else {
        recursive_other(fail, depth - 1, count) + 1
    }
}

fn recursive_other(fail: bool, depth: u32, count: &mut u32) -> u32 {
    if depth == 0 {
        middle(fail, count)
    } else {
        recursive(fail, depth - 1, count) + 1
    }
}

fn entry(fail: bool, deep: bool, count: &mut u32) -> u32 {
    recursive(fail, if deep { 3 } else { 2 }, count) + 1
}

fn main() {
    let fail = ::std::env::args().nth(1).as_deref() == Some("fail");
    let mut count = 0;
    let value = entry(fail, false, &mut count);
    let output = if value == 24 && count == 3 {
        "success\n"
    } else {
        "wrong\n"
    };
    let _ = ::std::io::Write::write_all(&mut ::std::io::stdout(), output.as_bytes());
}

//! Repeated, early-return, and exact-tail calls to a newly fallible callee.

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
    operation(fail).unwrap()
}

fn caller(fail: bool, mode: u32, count: &mut u32) -> u32 {
    if mode == 1 {
        return leaf(fail, count);
    }
    if mode == 2 {
        leaf(fail, count)
    } else {
        leaf(fail, count) + leaf(false, count)
    }
}

fn main() {
    let fail = ::std::env::args().any(|argument| argument == "fail");
    let early = ::std::env::args().any(|argument| argument == "early");
    let tail = ::std::env::args().any(|argument| argument == "tail");
    let mode = if early {
        1
    } else if tail {
        2
    } else {
        0
    };
    let mut count = 0;
    let value = caller(fail, mode, &mut count);
    println!("{value:?} {count}");
}

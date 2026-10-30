//! Fresh input for isolated construction of an owned, typed source chain.

fn operation(fail: bool) -> ::std::result::Result<u32, ::std::io::Error> {
    if fail {
        Err(::std::io::Error::from(
            ::std::io::ErrorKind::PermissionDenied,
        ))
    } else {
        Ok(7)
    }
}

fn leaf(fail: bool) -> u32 {
    operation(fail).unwrap()
}

fn main() {
    let fail = std::env::args().any(|argument| argument == "fail");
    println!("{}", leaf(fail) + 1);
}

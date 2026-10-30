//! Isolate failure-site propagation from return lifting and caller adaptation.

fn operation(fail: bool) -> ::std::result::Result<u32, ::std::io::Error> {
    if fail {
        Err(::std::io::Error::from(
            ::std::io::ErrorKind::PermissionDenied,
        ))
    } else {
        Ok(7)
    }
}

fn leaf(fail: bool) -> ::core::result::Result<u32, ::std::io::Error> {
    Ok(operation(fail).unwrap() + 1)
}

fn main() {
    let fail = std::env::args().any(|argument| argument == "fail");
    println!("{:?}", leaf(fail));
}

//! Isolate success-path lifting to a newly generated error contract.

fn operation(fail: bool) -> ::std::result::Result<u32, ::std::io::Error> {
    if fail {
        Err(::std::io::Error::from(
            ::std::io::ErrorKind::PermissionDenied,
        ))
    } else {
        Ok(7)
    }
}

fn leaf(fail: bool, early: bool) -> u32 {
    if early {
        return 11;
    }
    operation(fail).unwrap() + 1
}

fn main() {
    let fail = std::env::args().any(|argument| argument == "fail");
    let early = std::env::args().any(|argument| argument == "early");
    println!("{:?}", leaf(fail, early));
}

//! Fresh construction input: no retained error architecture or Cordial markers.

fn operation(fail: bool) -> std::result::Result<u32, std::io::Error> {
    if fail {
        Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
    } else {
        Ok(7)
    }
}

fn leaf(fail: bool) -> u32 {
    operation(fail).unwrap()
}

fn caller(fail: bool) -> u32 {
    leaf(fail) + 1
}

fn main() {
    let fail = std::env::args().any(|argument| argument == "fail");
    println!("{}", caller(fail));
}

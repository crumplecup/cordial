//! One failure-site operation, with observable evaluation order and source error.

fn operation(
    fail: bool,
    trace: &mut Vec<&'static str>,
) -> ::std::result::Result<u32, ::std::io::Error> {
    trace.push("source");
    if fail {
        Err(::std::io::Error::from(
            ::std::io::ErrorKind::PermissionDenied,
        ))
    } else {
        Ok(7)
    }
}

fn after(trace: &mut Vec<&'static str>) -> u32 {
    trace.push("after");
    1
}

fn leaf(
    fail: bool,
    trace: &mut Vec<&'static str>,
) -> ::core::result::Result<u32, ::std::io::Error> {
    trace.push("before");
    Ok(
        operation(fail, trace) /* receiver */ . /* dot */ unwrap /* name */ ( /* arguments */ )
            + after(trace),
    )
}

fn main() {
    let fail = std::env::args().any(|argument| argument == "fail");
    let mut trace = Vec::new();
    let result = leaf(fail, &mut trace);
    println!("{result:?}|{trace:?}");
}

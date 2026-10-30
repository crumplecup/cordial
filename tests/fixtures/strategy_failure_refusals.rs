type LocalError = ::std::io::Error;

fn source() -> ::std::result::Result<u32, ::std::io::Error> { Ok(7) }
fn optional() -> Option<u32> { Some(7) }
fn mismatch() -> ::std::result::Result<u32, ::std::fmt::Error> { Ok(source().unwrap()) }
fn alias() -> ::std::result::Result<u32, LocalError> { Ok(source().unwrap()) }
fn relative() -> std::result::Result<u32, std::io::Error> { Ok(source().unwrap()) }
fn not_result() -> ::std::result::Result<u32, ::std::io::Error> { Ok(optional().unwrap()) }
fn shadowed(source: fn() -> ::std::result::Result<u32, ::std::io::Error>) -> ::std::result::Result<u32, ::std::io::Error> { Ok(source().unwrap()) }
fn parenthesized() -> ::std::result::Result<u32, ::std::io::Error> { Ok((source()).unwrap()) }
fn nested() -> ::std::result::Result<u32, ::std::io::Error> { let inner = || source().unwrap(); Ok(inner()) }
fn main() {}

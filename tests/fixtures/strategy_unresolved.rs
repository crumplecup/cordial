type Alias = ::std::result::Result<u32, ::std::io::Error>;
fn relative() -> std::result::Result<u32, std::io::Error> { Ok(7) }
fn alias() -> Alias { Ok(7) }
fn unknown_error<E>() -> ::std::result::Result<u32, E> { Ok(7) }
fn shadowed(operation: fn() -> u32) -> u32 { operation() }
fn nested() -> u32 { let inner = || relative().unwrap(); inner() }
fn main() {}

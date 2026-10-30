fn source() -> ::std::result::Result<u32, ::std::io::Error> { Ok(7) }
fn leaf() -> u32 { source().unwrap() }
fn caller() -> u32 { leaf() + leaf() }
fn recurse(stop: bool) -> u32 { if stop { 0 } else { recurse(true) } }
fn shadow(source: fn() -> u32) -> u32 { source() }
fn local() -> u32 { let source = || 9; source() }
fn external() -> u32 { unavailable() }
fn qualified() -> u32 { crate::leaf() }
fn scopes() -> u32 { let hidden = || source().unwrap(); hidden() }
fn generic<T>() -> u32 { 1 }
fn calls_generic() -> u32 { generic::<u32>() }
fn main() {}

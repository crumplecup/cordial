fn source() -> ::std::result::Result<u32, ::std::io::Error> {
    Ok(7)
}

fn leaf() -> ::std::result::Result<u32, ::std::io::Error> {
    Ok(source().unwrap() + source().unwrap())
}

fn main() {
    println!("{:?}", leaf());
}

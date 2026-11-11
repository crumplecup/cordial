#[derive(Debug)]
pub struct Shown(u32);

pub struct Hidden(u32);

impl Shown {
    pub fn new() -> Self { Shown(0) }
}

impl Hidden {
    pub fn try_new() -> Self { Hidden(0) }
}

pub type Name<'a> = &'a str;

pub fn read_len(path: &str) -> Result<usize, std::io::Error> { Ok(path.len()) }

pub fn read_name(path: &str) -> Result<&str, std::io::Error> { Ok(path) }

pub fn read_alias(path: &str) -> Result<Name<'_>, std::io::Error> { Ok(path) }

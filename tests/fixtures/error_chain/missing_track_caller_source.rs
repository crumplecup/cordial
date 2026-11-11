
use std::io;

pub struct Error {
    kind: Box<ErrorKind>,
}

pub enum ErrorKind {
    Io(IoSource),
}

pub struct IoSource {
    source: io::Error,
    file: &'static str,
    line: u32,
}

impl IoSource {
    pub fn new(source: io::Error, file: &'static str, line: u32) -> Self {
        Self { source, file, line }
    }
}

impl std::error::Error for Error {}
impl std::error::Error for IoSource {}

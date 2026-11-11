
use std::io;
use std::panic::Location;

pub struct Error {
    kind: Box<ErrorKind>,
}

pub enum ErrorKind {
    Io(IoSource),
}

pub struct IoSource {
    source: io::Error,
    file: String,
    line: u32,
}

impl IoSource {
    #[track_caller]
    pub fn new(source: io::Error) -> Self {
        let loc = Location::caller();
        Self {
            source,
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl From<io::Error> for IoSource {
    #[track_caller]
    fn from(source: io::Error) -> Self {
        Self::new(source)
    }
}

impl std::error::Error for Error {}
impl std::error::Error for IoSource {}

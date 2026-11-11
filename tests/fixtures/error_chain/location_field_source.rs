
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
    location: &'static Location<'static>,
}

impl IoSource {
    #[track_caller]
    pub fn new(source: io::Error) -> Self {
        Self {
            source,
            location: Location::caller(),
        }
    }
}

impl std::error::Error for Error {}
impl std::error::Error for IoSource {}

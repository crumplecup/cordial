use std::error::Error;

type DynResult<T> = Result<T, Box<dyn Error>>;

struct Holds {
    err: Box<dyn std::error::Error + Send + Sync>,
}

fn returns_err() -> Box<dyn std::error::Error> {
    todo!()
}

fn accepts(e: Box<dyn Error>) {}

fn source_ref() -> Option<&'static dyn std::error::Error> {
    None
}

struct Diagnostic;

impl Diagnostic {
    fn code(&self) -> Option<Box<dyn std::fmt::Display>> {
        None
    }
}

use std::io;

use cordial::{CordialError, CordialErrorKind, CordialResult};

fn io_location(error: &CordialError) -> miette::Result<(&str, u32)> {
    match error.kind() {
        CordialErrorKind::Io(source) => Ok((source.file(), source.line())),
        other => Err(miette::miette!("expected I/O source, got {other:?}")),
    }
}

fn source_line(needle: &str) -> miette::Result<u32> {
    include_str!("error_caller_location.rs")
        .lines()
        .position(|line| line.contains(needle))
        .map(|index| index as u32 + 1)
        .ok_or_else(|| miette::miette!("missing source line: {needle}"))
}

fn io_error(message: &str) -> Result<(), io::Error> {
    Err(io::Error::other(message))
}

#[test]
fn caller_location_survives_question_mark_and_map_err_closure() -> miette::Result<()> {
    cordial::init_tracing();
    let bare = (|| -> CordialResult<()> {
        Err(io::Error::other("bare"))?;
        Ok(())
    })()
    .err()
    .ok_or_else(|| miette::miette!("expected bare error"))?;
    assert_eq!(
        io_location(&bare)?,
        (file!(), source_line("Err(io::Error::other(\"bare\"))?;")?)
    );

    let closure = io_error("closure")
        .map_err(|err| {
            assert_eq!(err.kind(), io::ErrorKind::Other);
            CordialError::from(err)
        })
        .err()
        .ok_or_else(|| miette::miette!("expected closure error"))?;
    assert_eq!(
        io_location(&closure)?,
        (file!(), source_line("CordialError::from(err)")?)
    );

    let item = io_error("item")
        .map_err(CordialError::from)
        .err()
        .ok_or_else(|| miette::miette!("expected item error"))?;
    assert_ne!(io_location(&item)?.0, file!());
    Ok(())
}

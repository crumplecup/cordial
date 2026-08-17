use std::io;

#[derive(Debug)]
struct IoSource {
    source: io::Error,
}

#[derive(Debug)]
enum UmbrellaKind {
    Io(IoSource),
}

impl From<io::Error> for IoSource {
    fn from(source: io::Error) -> Self {
        Self { source }
    }
}

impl From<IoSource> for UmbrellaKind {
    fn from(value: IoSource) -> Self {
        Self::Io(value)
    }
}

fn preserved_direct() -> Result<(), UmbrellaKind> {
    std::fs::read_to_string("x")?;
    Ok(())
}

fn preserved_map_err_from() -> Result<(), UmbrellaKind> {
    std::fs::read_to_string("x").map_err(IoSource::from)?;
    Ok(())
}

fn preserved_map_err_into() -> Result<(), UmbrellaKind> {
    std::fs::read_to_string("x").map_err(Into::into)?;
    Ok(())
}

fn preserved_map_err_closure() -> Result<(), UmbrellaKind> {
    std::fs::read_to_string("x").map_err(|e| UmbrellaKind::Io(IoSource { source: e }))?;
    Ok(())
}

fn broken_stringify() -> Result<(), UmbrellaKind> {
    std::fs::read_to_string("x").map_err(|e| UmbrellaKind::Io(IoSource {
        source: io::Error::new(io::ErrorKind::Other, e.to_string()),
    }))?;
    Ok(())
}

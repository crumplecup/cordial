#[cfg(any(feature = "antipatterns", feature = "dependency_freshness"))]
use std::error::Error as _;
use std::io;
use std::time::{Duration, UNIX_EPOCH};

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

#[test]
#[cfg(any(feature = "antipatterns", feature = "dependency_freshness"))]
fn clock_and_toml_errors_keep_typed_sources() -> miette::Result<()> {
    cordial::init_tracing();
    let clock = (|| -> CordialResult<()> {
        UNIX_EPOCH.duration_since(UNIX_EPOCH + Duration::from_secs(1))?;
        Ok(())
    })()
    .err()
    .ok_or_else(|| miette::miette!("expected clock error"))?;
    assert!(matches!(clock.kind(), CordialErrorKind::Clock(_)));
    assert!(
        clock
            .source()
            .and_then(std::error::Error::source)
            .and_then(|source| source.downcast_ref::<std::time::SystemTimeError>())
            .is_some()
    );

    let parse = toml::from_str::<toml::Table>("invalid = [")
        .err()
        .ok_or_else(|| miette::miette!("expected TOML parse error"))?;
    let wrapped = CordialError::toml_parse("broken.toml", parse);
    assert!(matches!(wrapped.kind(), CordialErrorKind::TomlParse(_)));
    assert!(wrapped.to_string().contains("broken.toml"));
    assert!(
        wrapped
            .source()
            .and_then(std::error::Error::source)
            .and_then(|source| source.downcast_ref::<toml::de::Error>())
            .is_some()
    );
    Ok(())
}

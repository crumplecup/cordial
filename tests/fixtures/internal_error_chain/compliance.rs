
use cordial::{CordialError, CordialResult};

fn stringify_foreign() -> CordialResult<()> {
    std::fs::read_to_string("x").map_err(|e| {
        std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
    })?;
    Ok(())
}

fn discard_typed() -> CordialResult<()> {
    std::fs::read_to_string("x").map_err(|e| CordialError::invariant(e.to_string()))?;
    Ok(())
}

fn if_let_discard(r: Result<i32, std::io::Error>) -> CordialResult<()> {
    if let Err(e) = r {
        return Err(CordialError::invariant(e.to_string()));
    }
    Ok(())
}

fn preserved() -> CordialResult<()> {
    std::fs::read_to_string("x")?;
    Ok(())
}

fn wrap_syn() -> CordialResult<()> {
    syn::parse_file("").map_err(|err| CordialError::syn_parse("x.rs", err))?;
    Ok(())
}

fn wrap_syn_display_path(path: &std::path::Path) -> CordialResult<()> {
    syn::parse_file("").map_err(|err| CordialError::syn_parse(path.display().to_string(), err))?;
    Ok(())
}

fn wrap_from() -> CordialResult<()> {
    std::fs::read_to_string("x").map_err(CordialError::from)?;
    Ok(())
}

fn wrap_json(path: &std::path::Path) -> CordialResult<()> {
    serde_json::from_str::<i32>("x")
        .map_err(|err| CordialError::json_parse(path.display().to_string(), err))?;
    Ok(())
}

fn wrap_cargo_metadata() -> CordialResult<()> {
    cargo_metadata::MetadataCommand::new()
        .exec()
        .map_err(CordialError::cargo_metadata)?;
    Ok(())
}

fn stringify_via_format() -> CordialResult<()> {
    serde_json::from_str::<i32>("x").map_err(|err| {
        CordialError::invariant(format!("parse failed: {err}"))
    })?;
    Ok(())
}

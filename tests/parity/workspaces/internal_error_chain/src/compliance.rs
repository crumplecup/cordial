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

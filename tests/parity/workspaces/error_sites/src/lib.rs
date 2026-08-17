use std::error::Error;

use cordial::{CordialError, CordialResult};

fn foreign_map_err() -> CordialResult<()> {
    std::fs::read_to_string("x").map_err(CordialError::from)?;
    Ok(())
}

fn propagate_internal(x: CordialResult<()>) -> CordialResult<()> {
    x?;
    Ok(())
}

fn return_internal() -> CordialResult<()> {
    return Err(CordialError::invariant("bad"));
}

fn if_let_foreign(r: Result<i32, std::io::Error>) -> CordialResult<()> {
    if let Err(e) = r {
        return Err(CordialError::from(e));
    }
    Ok(())
}

fn match_foreign(r: Result<i32, Box<dyn Error>>) -> CordialResult<()> {
    match r {
        Err(e) => return Err(CordialError::invariant(e.to_string())),
        Ok(_) => Ok(()),
    }
}

fn option_to_err(x: Option<i32>) -> CordialResult<i32> {
    x.ok_or_else(|| CordialError::invariant("missing"))
}

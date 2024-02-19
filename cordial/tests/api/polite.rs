use cordial::prelude::*;
use tracing::{info, warn};

pub fn fauxpas() -> Polite<()> {
    let _ = env()?;
    info!("Env test successful.");
    Ok(())
}

fn env() -> Polite<()> {
    match not_there() {
        Ok(not_there) => {
            warn!("Should not be reachable: {:#?}", not_there);
            Err(FauxPas::BadTest)
        }
        Err(FauxPas::Env(_)) => Ok(()),
        Err(e) => {
            warn!("Unexpected error: {:#?}", e.to_string());
            Err(FauxPas::BadTest)
        }
    }
}

fn not_there() -> Polite<String> {
    let not_there = std::env::var("NOT_THERE")?;
    Ok(not_there)
}

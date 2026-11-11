//! Reads what a `cordial` run printed and the worklist it left in the store.

use miette::IntoDiagnostic;
use std::process::Output;

pub fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

pub fn list(store: &cordial::StoreLayout) -> miette::Result<cordial::Worklist> {
    cordial::Worklist::read(store)
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("worklist missing"))
}

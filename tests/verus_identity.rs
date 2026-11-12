//! A `verus!` block is read because its macro is Verus's, not because of what it is called.
#![cfg(feature = "verus_ir")]

use cordial::{IrView, VerusCrateIr};
use miette::IntoDiagnostic;

#[path = "support/source_fixture.rs"]
mod source_fixture;

fn facts(source: &str) -> miette::Result<VerusCrateIr> {
    Ok(source_fixture::project_source(source)?
        .source_verus()
        .into_diagnostic()?
        .facts())
}

#[test]
fn a_renamed_import_of_the_verus_macro_is_read() -> miette::Result<()> {
    cordial::init_tracing();
    let ir = facts(
        "use verus_builtin_macros::verus as proofs;\n\nproofs! {\n\npub fn seen() {}\n\n}\n",
    )?;
    assert_eq!(ir.functions().len(), 1, "{:?}", ir.functions());
    Ok(())
}

#[test]
fn a_local_macro_named_verus_is_not_read() -> miette::Result<()> {
    cordial::init_tracing();
    let ir = facts(
        "macro_rules! verus { ($($tokens:tt)*) => {}; }\n\nverus! {\n\npub fn unseen() {}\n\n}\n",
    )?;
    assert!(ir.functions().is_empty(), "{:?}", ir.functions());
    Ok(())
}

#[test]
fn a_verus_block_without_the_macro_crate_is_not_read() -> miette::Result<()> {
    cordial::init_tracing();
    let ir = facts("verus! {\n\npub fn unseen() {}\n\n}\n")?;
    assert!(ir.functions().is_empty(), "{:?}", ir.functions());
    Ok(())
}

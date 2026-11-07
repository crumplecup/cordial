//! Return discovery keeps nested execution scopes separate from their parent.
//!
//! This specimen deliberately contains non-idiomatic control flow: a `loop` that
//! breaks with a value on its first pass, and an empty diverging `loop {}`. The
//! return transform must handle every valid Rust program, not only those that
//! pass `clippy -D warnings`, and `tests/strategy_return_lift.rs` and
//! `tests/strategy_specimens.rs` exercise these exact shapes. Rewriting them
//! would remove what the tests cover.
#![allow(clippy::never_loop, clippy::empty_loop)]
//
// EXCEPTION AUTHORIZED BY: Erik Rose (human), 2026-11-07. This `allow` is
// scoped to this specimen crate only. It is not precedent: agents must not add
// or widen `allow` attributes anywhere without explicit human authorization.

fn scoped(early: bool) -> u32 {
    let closure = || {
        if early {
            return 2;
        }
        3
    };
    fn local() -> u32 {
        4
    }
    let future = async { 5 };
    drop(future);
    let constant = const { 6 };
    if early {
        return closure();
    }
    local() + constant
}

pub const fn fixed_contract() -> u32 {
    1
}

fn residual() -> Result<u32, std::convert::Infallible> {
    Ok::<u32, std::convert::Infallible>(7)?;
    Ok(8)
}

fn macro_value() -> usize {
    stringify!(value).len()
}

fn loop_value() -> u32 {
    loop {
        break 5;
    }
}

fn diverge() -> ! {
    loop {}
}

fn identity<T>(value: T) -> T {
    value
}

#[inline]
fn attributed() -> u32 {
    1
}

fn main() {
    let _ = (macro_value(), loop_value(), identity(1), attributed());
    let _: fn() -> ! = diverge;
    let early = std::env::args().any(|argument| argument == "early");
    println!("{} {} {:?}", scoped(early), fixed_contract(), residual());
}

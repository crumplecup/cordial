//! Independent success adapters, observed through a Debug boundary.
//!
//! This specimen deliberately contains non-idiomatic return shapes: tail-position
//! `return`s in `if`/`else` chains and `match` arms, `return if ..`, and a
//! `let` immediately returned. The success-exit transform must handle every valid
//! Rust program, not only those that pass `clippy -D warnings`, and
//! `tests/strategy_return_paths.rs` asserts these exact shapes. Rewriting them
//! into idiomatic forms would remove what the test covers.
#![allow(clippy::needless_return, clippy::let_and_return)]
//
// EXCEPTION AUTHORIZED BY: Erik Rose (human), 2026-11-07. This `allow` is
// scoped to this specimen crate only. It is not precedent: agents must not add
// or widen `allow` attributes anywhere without explicit human authorization.

fn branch(which: u8) -> u32 {
    if which == 0 {
        return 7;
    } else if which == 1 {
        11
    } else {
        13
    }
}

fn all_returns(early: bool) -> u32 {
    if early {
        return 17;
    } else {
        return 19;
    };
}

fn matching(which: u8) -> u32 {
    match which {
        0 => return 23,
        1 => {
            let value = 29;
            value
        }
        _ => 31,
    }
}

fn unit(early: bool) {
    if early {
        return;
    };
}

fn tail_unit(early: bool) {
    if early {
        return;
    }
}

fn empty_unit() {}

fn unicode() -> &'static str {
    "é"
}

fn counted(counter: &mut u32, early: bool) -> u32 {
    *counter += 1;
    if early {
        return {
            *counter += 10;
            *counter
        };
    }
    {
        *counter += 100;
        *counter
    }
}

fn returned_branch(early: bool) -> u32 {
    return if early {
        return 37;
    } else {
        41
    };
}

fn main() {
    println!("branch: {:?} {:?} {:?}", branch(0), branch(1), branch(2));
    println!(
        "all_returns: {:?} {:?}",
        all_returns(true),
        all_returns(false)
    );
    println!(
        "matching: {:?} {:?} {:?}",
        matching(0),
        matching(1),
        matching(2)
    );
    println!("unit: {:?} {:?}", unit(true), unit(false));
    println!("tail_unit: {:?} {:?}", tail_unit(true), tail_unit(false));
    println!("empty_unit: {:?}", empty_unit());
    println!("unicode: {:?}", unicode());
    let mut counter = 0;
    let early = counted(&mut counter, true);
    let late = counted(&mut counter, false);
    println!("counted: {early:?} {late:?} {counter}");
    println!(
        "returned_branch: {:?} {:?}",
        returned_branch(true),
        returned_branch(false)
    );
}

//! Return discovery keeps nested execution scopes separate from their parent.

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

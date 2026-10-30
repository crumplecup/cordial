//! Written uses that cannot be recovered from direct free-function call edges.

fn leaf(value: u32) -> u32 {
    value + 1
}

const POINTER: fn(u32) -> u32 = leaf;

struct Caller;

impl Caller {
    fn invoke(value: u32) -> u32 {
        leaf(value)
    }
}

mod nested {
    pub fn invoke(value: u32) -> u32 {
        crate::leaf(value)
    }
}

fn entry() -> u32 {
    fn local(value: u32) -> u32 {
        leaf(value)
    }
    let function: fn(u32) -> u32 = leaf;
    let qualified: fn(u32) -> u32 = crate::leaf;
    let closure = |value| leaf(value);
    let future = async { leaf(6) };
    ::std::mem::drop(future);
    let constant = const { leaf as fn(u32) -> u32 };
    r#leaf(0)
        + function(1)
        + qualified(2)
        + closure(3)
        + local(4)
        + Caller::invoke(5)
        + constant(6)
        + nested::invoke(7)
        + POINTER(8)
}

fn main() {
    println!("{}", entry());
}

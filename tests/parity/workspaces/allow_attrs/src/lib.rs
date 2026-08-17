#![allow(dead_code)]

#[allow(clippy::too_many_arguments)]
fn many_args(_: u8, _: u8, _: u8, _: u8, _: u8) {}

struct Hidden;

struct Wrapper {
    #[allow(unused)]
    field: u8,
}

mod inner {
    #[allow(unused_variables)]
    fn unused_binding(value: u8) {
        let _ = value;
    }
}

fn clean_fn() {}

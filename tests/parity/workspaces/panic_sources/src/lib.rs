pub fn explodes() {
    panic!("boom");
}

pub fn not_possible() -> ! {
    unreachable!("not possible");
}

pub fn expects(x: Option<i32>) -> i32 {
    x.expect("missing value")
}

pub fn unwraps(x: Option<i32>) -> i32 {
    x.unwrap()
}

pub fn compile_fail() {
    compile_error!("intentional fixture");
}


fn symbolic_any() -> i32 {
    #[cfg(kani)]
    {
        0
    }

    #[cfg(not(kani))]
    {
        panic!("symbolic construction is only available under cfg(kani)")
    }
}

#[kani::proof]
fn verify_something() {
    let _ = symbolic_any();
}

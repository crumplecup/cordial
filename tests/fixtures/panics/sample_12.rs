
fn discards_the_error(res: Result<i32, i32>) {
    let _error = res.expect_err("must fail");
}

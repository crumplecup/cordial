
fn logs_the_error(res: Result<i32, i32>) {
    let error = res.expect_err("must fail");
    println!("{error}");
}

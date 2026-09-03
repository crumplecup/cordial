fn take(res: Result<i32, i32>) -> i32 {
    res.unwrap_err()
}

fn take2(res: Result<i32, i32>) -> i32 {
    res.expect_err("must be an error")
}

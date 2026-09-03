
fn validate_rejects_a_negative_amount(res: Result<i32, TransferError>) {
    let error = res.expect_err("negative amount");
    assert_eq!(error, TransferError::NegativeAmount(-1));
}

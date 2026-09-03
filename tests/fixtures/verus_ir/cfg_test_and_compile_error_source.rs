
use verus_builtin_macros::verus;

verus! {

pub fn in_library_code() -> (result: bool)
{
    compile_error!("should never reach exec");
    true
}

}

#[cfg(test)]
mod tests {
    use verus_builtin_macros::verus;

    verus! {

    pub fn in_test_code() -> (result: bool)
    {
        true
    }

    }
}
